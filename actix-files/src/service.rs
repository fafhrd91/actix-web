use std::{fmt, io, path::PathBuf, rc::Rc};

use actix_service::Service;
use actix_utils::future::ok;
use actix_web::{
    dev::{ServiceRequest, ServiceResponse},
    error::Error,
    guard::Guard,
    http::{header, Method},
    HttpResponse,
};
use futures_core::future::LocalBoxFuture;

use crate::{
    named, Directory, DirectoryRenderer, FilesError, HttpService, MimeOverride, NamedFile,
    PathBufWrap, PathFilter,
};

/// Assembled file serving service.
pub struct FilesService {
    pub(crate) directory: PathBuf,
    pub(crate) index: Option<String>,
    pub(crate) show_index: bool,
    pub(crate) redirect_to_slash: bool,
    pub(crate) default: Option<HttpService>,
    pub(crate) renderer: Rc<DirectoryRenderer>,
    pub(crate) mime_override: Option<Rc<MimeOverride>>,
    pub(crate) path_filter: Option<Rc<PathFilter>>,
    pub(crate) file_flags: named::Flags,
    pub(crate) guards: Option<Rc<dyn Guard>>,
    pub(crate) hidden_files: bool,
}

impl FilesService {
    fn handle_err(
        &self,
        err: io::Error,
        req: ServiceRequest,
    ) -> LocalBoxFuture<'static, Result<ServiceResponse, Error>> {
        log::debug!("error handling {}: {}", req.path(), err);

        if let Some(ref default) = self.default {
            Box::pin(default.call(req))
        } else {
            Box::pin(ok(req.error_response(err)))
        }
    }
}

impl fmt::Debug for FilesService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FilesService")
    }
}

impl Service<ServiceRequest> for FilesService {
    type Response = ServiceResponse;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<ServiceResponse, Error>>;

    actix_service::always_ready!();

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let is_method_valid = if let Some(guard) = &self.guards {
            // execute user defined guards
            (**guard).check(req.head())
        } else {
            // default behavior
            matches!(*req.method(), Method::HEAD | Method::GET)
        };

        if !is_method_valid {
            return Box::pin(ok(req.into_response(
                actix_web::HttpResponse::MethodNotAllowed()
                    .insert_header(header::ContentType(mime::TEXT_PLAIN_UTF_8))
                    .body("Request did not meet this resource's requirements."),
            )));
        }

        let real_path =
            match PathBufWrap::parse_path(req.match_info().path(), self.hidden_files) {
                Ok(item) => item,
                Err(e) => return Box::pin(ok(req.error_response(e))),
            };

        if let Some(filter) = &self.path_filter {
            if !filter(real_path.as_ref(), req.head()) {
                if let Some(ref default) = self.default {
                    return Box::pin(default.call(req));
                } else {
                    return Box::pin(ok(
                        req.into_response(actix_web::HttpResponse::NotFound().finish())
                    ));
                }
            }
        }

        // full file path
        let path = self.directory.join(&real_path);
        if let Err(err) = path.canonicalize() {
            return Box::pin(self.handle_err(err, req));
        }

        let is_path_descendant = path
            .as_path()
            .ancestors()
            .any(|ancestor| ancestor == self.directory.as_path());
        if !is_path_descendant {
            return Either::Left(ok(req.into_response(
                actix_web::HttpResponse::Forbidden()
                    .insert_header(header::ContentType(mime::TEXT_PLAIN_UTF_8))
                    .body("Requested files outside the server."),
            )));
        }

        if path.is_dir() {
            if self.redirect_to_slash
                && !req.path().ends_with('/')
                && (self.index.is_some() || self.show_index)
            {
                let redirect_to = format!("{}/", req.path());

                return Box::pin(ok(req.into_response(
                    HttpResponse::Found()
                        .insert_header((header::LOCATION, redirect_to))
                        .finish(),
                )));
            }

            let serve_named_file = |req: ServiceRequest, mut named_file: NamedFile| {
                if let Some(ref mime_override) = self.mime_override {
                    let new_disposition = mime_override(&named_file.content_type.type_());
                    named_file.content_disposition.disposition = new_disposition;
                }
                named_file.flags = self.file_flags;

                let (req, _) = req.into_parts();
                let res = named_file.into_response(&req);
                Box::pin(ok(ServiceResponse::new(req, res)))
            };

            let show_index = |req: ServiceRequest| {
                let dir = Directory::new(self.directory.clone(), path.clone());

                let (req, _) = req.into_parts();
                let x = (self.renderer)(&dir, &req);

                Box::pin(match x {
                    Ok(resp) => ok(resp),
                    Err(err) => ok(ServiceResponse::from_err(err, req)),
                })
            };

            match self.index {
                Some(ref index) => match NamedFile::open(path.join(index)) {
                    Ok(named_file) => serve_named_file(req, named_file),
                    Err(_) if self.show_index => show_index(req),
                    Err(err) => self.handle_err(err, req),
                },
                None if self.show_index => show_index(req),
                _ => Box::pin(ok(ServiceResponse::from_err(
                    FilesError::IsDirectory,
                    req.into_parts().0,
                ))),
            }
        } else {
            match NamedFile::open(path) {
                Ok(mut named_file) => {
                    if let Some(ref mime_override) = self.mime_override {
                        let new_disposition = mime_override(&named_file.content_type.type_());
                        named_file.content_disposition.disposition = new_disposition;
                    }
                    named_file.flags = self.file_flags;

                    let (req, _) = req.into_parts();
                    let res = named_file.into_response(&req);
                    Box::pin(ok(ServiceResponse::new(req, res)))
                }
                Err(err) => self.handle_err(err, req),
            }
        }
    }
}
