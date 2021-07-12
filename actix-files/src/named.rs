use actix_service::{Service, ServiceFactory};
use actix_utils::future::{ok, ready, Ready};
use actix_web::dev::{AppService, HttpServiceFactory, ResourceDef};
use std::fs::{File, Metadata};
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;

use actix_web::{
    dev::{BodyEncoding, ServiceRequest, ServiceResponse, SizedStream},
    http::{
        header::{
            self, Charset, ContentDisposition, DispositionParam, DispositionType, ExtendedValue,
        },
        ContentEncoding, StatusCode,
    },
    Error, HttpMessage, HttpRequest, HttpResponse, Responder,
};
use bitflags::bitflags;
use mime_guess::from_path;

use crate::ChunkedReadFile;
use crate::{encoding::equiv_utf8_text, range::HttpRange};

bitflags! {
    pub(crate) struct Flags: u8 {
        const ETAG =                0b0000_0001;
        const LAST_MD =             0b0000_0010;
        const CONTENT_DISPOSITION = 0b0000_0100;
        const PREFER_UTF8 =         0b0000_1000;
    }
}

impl Default for Flags {
    fn default() -> Self {
        Flags::from_bits_truncate(0b0000_0111)
    }
}

/// A file with an associated name.
///
/// `NamedFile` can be registered as services:
/// ```
/// use actix_web::App;
/// use actix_files::NamedFile;
///
/// # fn run() -> Result<(), Box<dyn std::error::Error>> {
/// let app = App::new()
///     .service(NamedFile::open("./static/index.html")?);
/// # Ok(())
/// # }
/// ```
///
/// They can also be returned from handlers:
/// ```
/// use actix_web::{Responder, get};
/// use actix_files::NamedFile;
///
/// #[get("/")]
/// async fn index() -> impl Responder {
///     NamedFile::open("./static/index.html")
/// }
/// ```
#[derive(Debug)]
pub struct NamedFile {
    path: PathBuf,
    file: File,
    modified: Option<SystemTime>,
    pub(crate) md: Metadata,
    pub(crate) flags: Flags,
    pub(crate) status_code: StatusCode,
    pub(crate) content_type: mime::Mime,
    pub(crate) content_disposition: header::ContentDisposition,
    pub(crate) encoding: Option<ContentEncoding>,
}

impl NamedFile {
    /// Creates an instance from a previously opened file.
    ///
    /// The given `path` need not exist and is only used to determine the `ContentType` and
    /// `ContentDisposition` headers.
    ///
    /// # Examples
    ///
    /// ```
    /// use actix_files::NamedFile;
    /// use std::io::{self, Write};
    /// use std::env;
    /// use std::fs::File;
    ///
    /// fn main() -> io::Result<()> {
    ///     let mut file = File::create("foo.txt")?;
    ///     file.write_all(b"Hello, world!")?;
    ///     let named_file = NamedFile::from_file(file, "bar.txt")?;
    ///     # std::fs::remove_file("foo.txt");
    ///     Ok(())
    /// }
    /// ```
    pub fn from_file<P: AsRef<Path>>(file: File, path: P) -> io::Result<NamedFile> {
        let path = path.as_ref().to_path_buf();

        // Get the name of the file and use it to construct default Content-Type
        // and Content-Disposition values
        let (content_type, content_disposition) = {
            let filename = match path.file_name() {
                Some(name) => name.to_string_lossy(),
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Provided path has no filename",
                    ));
                }
            };

            let ct = from_path(&path).first_or_octet_stream();

            let disposition = match ct.type_() {
                mime::IMAGE | mime::TEXT | mime::VIDEO => DispositionType::Inline,
                mime::APPLICATION => match ct.subtype() {
                    mime::JAVASCRIPT | mime::JSON => DispositionType::Inline,
                    name if name == "wasm" => DispositionType::Inline,
                    _ => DispositionType::Attachment,
                },
                _ => DispositionType::Attachment,
            };

            let mut parameters =
                vec![DispositionParam::Filename(String::from(filename.as_ref()))];

            if !filename.is_ascii() {
                parameters.push(DispositionParam::FilenameExt(ExtendedValue {
                    charset: Charset::Ext(String::from("UTF-8")),
                    language_tag: None,
                    value: filename.into_owned().into_bytes(),
                }))
            }

            let cd = ContentDisposition {
                disposition,
                parameters,
            };

            (ct, cd)
        };

        let md = file.metadata()?;
        let modified = md.modified().ok();
        let encoding = None;

        Ok(NamedFile {
            path,
            file,
            content_type,
            content_disposition,
            md,
            modified,
            encoding,
            status_code: StatusCode::OK,
            flags: Flags::default(),
        })
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// # Examples
    ///
    /// ```
    /// use actix_files::NamedFile;
    ///
    /// let file = NamedFile::open("foo.txt");
    /// ```
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<NamedFile> {
        Self::from_file(File::open(&path)?, path)
    }

    /// Returns reference to the underlying `File` object.
    #[inline]
    pub fn file(&self) -> &File {
        &self.file
    }

    /// Retrieve the path of this file.
    ///
    /// # Examples
    ///
    /// ```
    /// # use std::io;
    /// use actix_files::NamedFile;
    ///
    /// # fn path() -> io::Result<()> {
    /// let file = NamedFile::open("test.txt")?;
    /// assert_eq!(file.path().as_os_str(), "foo.txt");
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    /// Retrieve the time the file was modified.
    #[inline]
    pub fn modified(&self) -> Option<&SystemTime> {
        self.modified.as_ref()
    }

    /// Retrieve the metadata associated with this file.
    #[inline]
    pub fn md(&self) -> &Metadata {
        &self.md
    }

    /// Retrieve the value of the `ContentType` header for serving this file.
    #[inline]
    pub fn content_type(&self) -> &mime::Mime {
        &self.content_type
    }

    /// Retrieve the value of the `ContentDisposition` header for serving this file.
    #[inline]
    pub fn content_disposition(&self) -> &header::ContentDisposition {
        &self.content_disposition
    }

    /// Retrieve the content encoding for serving this file.
    #[inline]
    pub fn encoding(&self) -> Option<&ContentEncoding> {
        self.encoding.as_ref()
    }

    /// Retrieve the status code for serving this file.
    #[inline]
    pub fn status_code(&self) -> &StatusCode {
        &self.status_code
    }

    /// Set response **Status Code**
    pub fn set_status_code(mut self, status: StatusCode) -> Self {
        self.status_code = status;
        self
    }

    /// Set the MIME Content-Type for serving this file. By default
    /// the Content-Type is inferred from the filename extension.
    #[inline]
    pub fn set_content_type(mut self, mime_type: mime::Mime) -> Self {
        self.content_type = mime_type;
        self
    }

    /// Set the Content-Disposition for serving this file. This allows
    /// changing the inline/attachment disposition as well as the filename
    /// sent to the peer.
    ///
    /// By default the disposition is `inline` for `text/*`, `image/*`, `video/*` and
    /// `application/{javascript, json, wasm}` mime types, and `attachment` otherwise,
    /// and the filename is taken from the path provided in the `open` method
    /// after converting it to UTF-8 using.
    /// [`std::ffi::OsStr::to_string_lossy`]
    #[inline]
    pub fn set_content_disposition(mut self, cd: header::ContentDisposition) -> Self {
        self.content_disposition = cd;
        self.flags.insert(Flags::CONTENT_DISPOSITION);
        self
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    #[inline]
    pub fn disable_content_disposition(mut self) -> Self {
        self.flags.remove(Flags::CONTENT_DISPOSITION);
        self
    }

    /// Set content encoding for serving this file
    ///
    /// Must be used with [`actix_web::middleware::Compress`] to take effect.
    #[inline]
    pub fn set_content_encoding(mut self, enc: ContentEncoding) -> Self {
        self.encoding = Some(enc);
        self
    }

    /// Specifies whether to use ETag or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_etag(mut self, value: bool) -> Self {
        self.flags.set(Flags::ETAG, value);
        self
    }

    /// Specifies whether to use Last-Modified or not.
    ///
    /// Default is true.
    #[inline]
    pub fn use_last_modified(mut self, value: bool) -> Self {
        self.flags.set(Flags::LAST_MD, value);
        self
    }

    /// Specifies whether text responses should signal a UTF-8 encoding.
    ///
    /// Default is false (but will default to true in a future version).
    #[inline]
    pub fn prefer_utf8(mut self, value: bool) -> Self {
        self.flags.set(Flags::PREFER_UTF8, value);
        self
    }

    pub(crate) fn etag(&self) -> Option<header::EntityTag> {
        // This etag format is similar to Apache's.
        self.modified.as_ref().map(|mtime| {
            let ino = {
                #[cfg(unix)]
                {
                    self.md.ino()
                }
                #[cfg(not(unix))]
                {
                    0
                }
            };

            let dur = mtime
                .duration_since(UNIX_EPOCH)
                .expect("modification time must be after epoch");

            header::EntityTag::strong(format!(
                "{:x}:{:x}:{:x}:{:x}",
                ino,
                self.md.len(),
                dur.as_secs(),
                dur.subsec_nanos()
            ))
        })
    }

    pub(crate) fn last_modified(&self) -> Option<header::HttpDate> {
        self.modified.map(|mtime| mtime.into())
    }

    /// Creates an `HttpResponse` with file as a streaming body.
    pub fn into_response(self, req: &HttpRequest) -> HttpResponse {
        if self.status_code != StatusCode::OK {
            let mut res = HttpResponse::build(self.status_code);

            if self.flags.contains(Flags::PREFER_UTF8) {
                let ct = equiv_utf8_text(self.content_type.clone());
                res.insert_header((header::CONTENT_TYPE, ct.to_string()));
            } else {
                res.insert_header((header::CONTENT_TYPE, self.content_type.to_string()));
            }

            if self.flags.contains(Flags::CONTENT_DISPOSITION) {
                res.insert_header((
                    header::CONTENT_DISPOSITION,
                    self.content_disposition.to_string(),
                ));
            }

            if let Some(current_encoding) = self.encoding {
                res.encoding(current_encoding);
            }

            let reader = ChunkedReadFile::new(self.md.len(), 0, self.file);

            return res.streaming(reader);
        }

        let etag = if self.flags.contains(Flags::ETAG) {
            self.etag()
        } else {
            None
        };

        let last_modified = if self.flags.contains(Flags::LAST_MD) {
            self.last_modified()
        } else {
            None
        };

        // check preconditions
        let precondition_failed = if !any_match(etag.as_ref(), req) {
            true
        } else if let (Some(ref m), Some(header::IfUnmodifiedSince(ref since))) =
            (last_modified, req.get_header())
        {
            let t1: SystemTime = (*m).into();
            let t2: SystemTime = (*since).into();

            match (t1.duration_since(UNIX_EPOCH), t2.duration_since(UNIX_EPOCH)) {
                (Ok(t1), Ok(t2)) => t1.as_secs() > t2.as_secs(),
                _ => false,
            }
        } else {
            false
        };

        // check last modified
        let not_modified = if !none_match(etag.as_ref(), req) {
            true
        } else if req.headers().contains_key(header::IF_NONE_MATCH) {
            false
        } else if let (Some(ref m), Some(header::IfModifiedSince(ref since))) =
            (last_modified, req.get_header())
        {
            let t1: SystemTime = (*m).into();
            let t2: SystemTime = (*since).into();

            match (t1.duration_since(UNIX_EPOCH), t2.duration_since(UNIX_EPOCH)) {
                (Ok(t1), Ok(t2)) => t1.as_secs() <= t2.as_secs(),
                _ => false,
            }
        } else {
            false
        };

        let mut resp = HttpResponse::build(self.status_code);

        if self.flags.contains(Flags::PREFER_UTF8) {
            let ct = equiv_utf8_text(self.content_type.clone());
            resp.insert_header((header::CONTENT_TYPE, ct.to_string()));
        } else {
            resp.insert_header((header::CONTENT_TYPE, self.content_type.to_string()));
        }

        if self.flags.contains(Flags::CONTENT_DISPOSITION) {
            resp.insert_header((
                header::CONTENT_DISPOSITION,
                self.content_disposition.to_string(),
            ));
        }

        // default compressing
        if let Some(current_encoding) = self.encoding {
            resp.encoding(current_encoding);
        }

        if let Some(lm) = last_modified {
            resp.insert_header((header::LAST_MODIFIED, lm.to_string()));
        }

        if let Some(etag) = etag {
            resp.insert_header((header::ETAG, etag.to_string()));
        }

        resp.insert_header((header::ACCEPT_RANGES, "bytes"));

        let mut length = self.md.len();
        let mut offset = 0;

        // check for range header
        if let Some(ranges) = req.headers().get(header::RANGE) {
            if let Ok(ranges_header) = ranges.to_str() {
                if let Ok(ranges) = HttpRange::parse(ranges_header, length) {
                    length = ranges[0].length;
                    offset = ranges[0].start;

                    resp.encoding(ContentEncoding::Identity);
                    resp.insert_header((
                        header::CONTENT_RANGE,
                        format!("bytes {}-{}/{}", offset, offset + length - 1, self.md.len()),
                    ));
                } else {
                    resp.insert_header((header::CONTENT_RANGE, format!("bytes */{}", length)));
                    return resp.status(StatusCode::RANGE_NOT_SATISFIABLE).finish();
                };
            } else {
                return resp.status(StatusCode::BAD_REQUEST).finish();
            };
        };

        if precondition_failed {
            return resp.status(StatusCode::PRECONDITION_FAILED).finish();
        } else if not_modified {
            return resp.status(StatusCode::NOT_MODIFIED).finish();
        }

        let reader = ChunkedReadFile::new(length, offset, self.file);

        if offset != 0 || length != self.md.len() {
            resp.status(StatusCode::PARTIAL_CONTENT);
        }

        resp.body(SizedStream::new(length, reader))
    }
}

impl Deref for NamedFile {
    type Target = File;

    fn deref(&self) -> &File {
        &self.file
    }
}

impl DerefMut for NamedFile {
    fn deref_mut(&mut self) -> &mut File {
        &mut self.file
    }
}

/// Returns true if `req` has no `If-Match` header or one which matches `etag`.
fn any_match(etag: Option<&header::EntityTag>, req: &HttpRequest) -> bool {
    match req.get_header::<header::IfMatch>() {
        None | Some(header::IfMatch::Any) => true,

        Some(header::IfMatch::Items(ref items)) => {
            if let Some(some_etag) = etag {
                for item in items {
                    if item.strong_eq(some_etag) {
                        return true;
                    }
                }
            }

            false
        }
    }
}

/// Returns true if `req` doesn't have an `If-None-Match` header matching `req`.
fn none_match(etag: Option<&header::EntityTag>, req: &HttpRequest) -> bool {
    match req.get_header::<header::IfNoneMatch>() {
        Some(header::IfNoneMatch::Any) => false,

        Some(header::IfNoneMatch::Items(ref items)) => {
            if let Some(some_etag) = etag {
                for item in items {
                    if item.weak_eq(some_etag) {
                        return false;
                    }
                }
            }

            true
        }

        None => true,
    }
}

impl Responder for NamedFile {
    fn respond_to(self, req: &HttpRequest) -> HttpResponse {
        self.into_response(req)
    }
}

impl ServiceFactory<ServiceRequest> for NamedFile {
    type Response = ServiceResponse;
    type Error = Error;
    type Config = ();
    type InitError = ();
    type Service = NamedFileService;
    type Future = Ready<Result<Self::Service, ()>>;

    fn new_service(&self, _: ()) -> Self::Future {
        ok(NamedFileService {
            path: self.path.clone(),
        })
    }
}

#[doc(hidden)]
#[derive(Debug)]
pub struct NamedFileService {
    path: PathBuf,
}

impl Service<ServiceRequest> for NamedFileService {
    type Response = ServiceResponse;
    type Error = Error;
    type Future = Ready<Result<Self::Response, Self::Error>>;

    actix_service::always_ready!();

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let (req, _) = req.into_parts();
        ready(
            NamedFile::open(&self.path)
                .map_err(|e| e.into())
                .map(|f| f.into_response(&req))
                .map(|res| ServiceResponse::new(req, res)),
        )
    }
}

impl HttpServiceFactory for NamedFile {
    fn register(self, config: &mut AppService) {
        config.register_service(
            ResourceDef::root_prefix(self.path.to_string_lossy().as_ref()),
            None,
            self,
            None,
        )
    }
}
