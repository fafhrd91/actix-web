use std::{
    cell::RefCell,
    fmt, io,
    path::{Path, PathBuf},
    rc::Rc,
};

use actix_service::{boxed, IntoServiceFactory, ServiceFactory, ServiceFactoryExt};
use actix_utils::future::ok;
use actix_web::{
    dev::{
        AppService, HttpServiceFactory, RequestHead, ResourceDef, ServiceRequest,
        ServiceResponse,
    },
    error::Error,
    guard::Guard,
    http::header::DispositionType,
    HttpRequest,
};
use futures_core::future::LocalBoxFuture;

use crate::{
    directory_listing, named, Directory, DirectoryRenderer, FilesService, HttpNewService,
    MimeOverride, PathFilter,
};

/// Static files handling service.
///
/// `Files` service must be registered with `App::service()` method.
///
/// ```
/// use actix_web::App;
/// use actix_files::Files;
///
/// let app = App::new()
///     .service(Files::new("/static", "."));
/// ```
pub struct Files {
    path: String,
    directory: PathBuf,
    index: Option<String>,
    show_index: bool,
    redirect_to_slash: bool,
    default: Rc<RefCell<Option<Rc<HttpNewService>>>>,
    renderer: Rc<DirectoryRenderer>,
    mime_override: Option<Rc<MimeOverride>>,
    path_filter: Option<Rc<PathFilter>>,
    file_flags: named::Flags,
    use_guards: Option<Rc<dyn Guard>>,
    guards: Vec<Rc<dyn Guard>>,
    hidden_files: bool,
}

impl fmt::Debug for Files {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Files")
    }
}

impl Clone for Files {
    fn clone(&self) -> Self {
        Self {
            directory: self.directory.clone(),
            index: self.index.clone(),
            show_index: self.show_index,
            redirect_to_slash: self.redirect_to_slash,
            default: self.default.clone(),
            renderer: self.renderer.clone(),
            file_flags: self.file_flags,
            path: self.path.clone(),
            mime_override: self.mime_override.clone(),
            path_filter: self.path_filter.clone(),
            use_guards: self.use_guards.clone(),
            guards: self.guards.clone(),
            hidden_files: self.hidden_files,
        }
    }
}

impl Files {
    /// Create new `Files` instance for a specified base directory.
    ///
    /// # Argument Order
    /// The first argument (`mount_path`) is the root URL at which the static files are served.
    /// For example, `/assets` will serve files at `example.com/assets/...`.
    ///
    /// The second argument (`serve_from`) is the location on disk at which files are loaded.
    /// This can be a relative path. For example, `./` would serve files from the current
    /// working directory. Requests for files outside of the `serve_from` directory
    /// will receive a Forbidden Response.
    ///
    /// # Implementation Notes
    /// If the mount path is set as the root path `/`, services registered after this one will
    /// be inaccessible. Register more specific handlers and services first.
    ///
    /// `Files` utilizes the existing Tokio thread-pool for blocking filesystem operations.
    /// The number of running threads is adjusted over time as needed, up to a maximum of 512 times
    /// the number of server [workers](actix_web::HttpServer::workers), by default.
    pub fn new<T: Into<PathBuf>>(mount_path: &str, serve_from: T) -> Files {
        let orig_dir = serve_from.into();
        let dir = match orig_dir.canonicalize() {
            Ok(canon_dir) => canon_dir,
            Err(_) => {
                log::error!("Specified path is not a directory: {:?}", orig_dir);
                PathBuf::new()
            }
        };

        Files {
            path: mount_path.to_owned(),
            directory: dir,
            index: None,
            show_index: false,
            redirect_to_slash: false,
            default: Rc::new(RefCell::new(None)),
            renderer: Rc::new(directory_listing),
            mime_override: None,
            path_filter: None,
            file_flags: named::Flags::default(),
            use_guards: None,
            guards: Vec::new(),
            hidden_files: false,
        }
    }

    /// Show files listing for directories.
    ///
    /// By default show files listing is disabled.
    ///
    /// When used with [`Files::index_file()`], files listing is shown as a fallback
    /// when the index file is not found.
    pub fn show_files_listing(mut self) -> Self {
        self.show_index = true;
        self
    }

    /// Redirects to a slash-ended path when browsing a directory.
    ///
    /// By default never redirect.
    pub fn redirect_to_slash_directory(mut self) -> Self {
        self.redirect_to_slash = true;
        self
    }

    /// Set custom directory renderer
    pub fn files_listing_renderer<F>(mut self, f: F) -> Self
    where
        for<'r, 's> F:
            Fn(&'r Directory, &'s HttpRequest) -> Result<ServiceResponse, io::Error> + 'static,
    {
        self.renderer = Rc::new(f);
        self
    }

    /// Specifies mime override callback
    pub fn mime_override<F>(mut self, f: F) -> Self
    where
        F: Fn(&mime::Name<'_>) -> DispositionType + 'static,
    {
        self.mime_override = Some(Rc::new(f));
        self
    }

    /// Sets path filtering closure.
    ///
    /// The path provided to the closure is relative to `serve_from` path.
    /// You can safely join this path with the `serve_from` path to get the real path.
    /// However, the real path may not exist since the filter is called before checking path existence.
    ///
    /// When a path doesn't pass the filter, [`Files::default_handler`] is called if set, otherwise,
    /// `404 Not Found` is returned.
    ///
    /// # Examples
    /// ```
    /// use std::path::Path;
    /// use actix_files::Files;
    ///
    /// // prevent searching subdirectories and following symlinks
    /// let files_service = Files::new("/", "./static").path_filter(|path, _| {
    ///     path.components().count() == 1
    ///         && Path::new("./static")
    ///             .join(path)
    ///             .symlink_metadata()
    ///             .map(|m| !m.file_type().is_symlink())
    ///             .unwrap_or(false)
    /// });
    /// ```
    pub fn path_filter<F>(mut self, f: F) -> Self
    where
        F: Fn(&Path, &RequestHead) -> bool + 'static,
    {
        self.path_filter = Some(Rc::new(f));
        self
    }

    /// Set index file
    ///
    /// Shows specific index file for directories instead of
    /// showing files listing.
    ///
    /// If the index file is not found, files listing is shown as a fallback if
    /// [`Files::show_files_listing()`] is set.
    pub fn index_file<T: Into<String>>(mut self, index: T) -> Self {
        self.index = Some(index.into());
        self
    }

    /// Specifies whether to use ETag or not.
    ///
    /// Default is true.
    pub fn use_etag(mut self, value: bool) -> Self {
        self.file_flags.set(named::Flags::ETAG, value);
        self
    }

    /// Specifies whether to use Last-Modified or not.
    ///
    /// Default is true.
    pub fn use_last_modified(mut self, value: bool) -> Self {
        self.file_flags.set(named::Flags::LAST_MD, value);
        self
    }

    /// Specifies whether text responses should signal a UTF-8 encoding.
    ///
    /// Default is false (but will default to true in a future version).
    pub fn prefer_utf8(mut self, value: bool) -> Self {
        self.file_flags.set(named::Flags::PREFER_UTF8, value);
        self
    }

    /// Adds a routing guard.
    ///
    /// Use this to allow multiple chained file services that respond to strictly different
    /// properties of a request. Due to the way routing works, if a guard check returns true and the
    /// request starts being handled by the file service, it will not be able to back-out and try
    /// the next service, you will simply get a 404 (or 405) error response.
    ///
    /// To allow `POST` requests to retrieve files, see [`Files::use_guards`].
    ///
    /// # Examples
    /// ```
    /// use actix_web::{guard::Header, App};
    /// use actix_files::Files;
    ///
    /// App::new().service(
    ///     Files::new("/","/my/site/files")
    ///         .guard(Header("Host", "example.com"))
    /// );
    /// ```
    pub fn guard<G: Guard + 'static>(mut self, guard: G) -> Self {
        self.guards.push(Rc::new(guard));
        self
    }

    /// Specifies guard to check before fetching directory listings or files.
    ///
    /// Note that this guard has no effect on routing; it's main use is to guard on the request's
    /// method just before serving the file, only allowing `GET` and `HEAD` requests by default.
    /// See [`Files::guard`] for routing guards.
    pub fn method_guard<G: Guard + 'static>(mut self, guard: G) -> Self {
        self.use_guards = Some(Rc::new(guard));
        self
    }

    #[doc(hidden)]
    #[deprecated(since = "0.6.0", note = "Renamed to `method_guard`.")]
    /// See [`Files::method_guard`].
    pub fn use_guards<G: Guard + 'static>(self, guard: G) -> Self {
        self.method_guard(guard)
    }

    /// Disable `Content-Disposition` header.
    ///
    /// By default Content-Disposition` header is enabled.
    pub fn disable_content_disposition(mut self) -> Self {
        self.file_flags.remove(named::Flags::CONTENT_DISPOSITION);
        self
    }

    /// Sets default handler which is used when no matched file could be found.
    ///
    /// # Examples
    /// Setting a fallback static file handler:
    /// ```
    /// use actix_files::{Files, NamedFile};
    ///
    /// # fn run() -> Result<(), actix_web::Error> {
    /// let files = Files::new("/", "./static")
    ///     .index_file("index.html")
    ///     .default_handler(NamedFile::open("./static/404.html")?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn default_handler<F, U>(mut self, f: F) -> Self
    where
        F: IntoServiceFactory<U, ServiceRequest>,
        U: ServiceFactory<
                ServiceRequest,
                Config = (),
                Response = ServiceResponse,
                Error = Error,
            > + 'static,
    {
        // create and configure default resource
        self.default = Rc::new(RefCell::new(Some(Rc::new(boxed::factory(
            f.into_factory().map_init_err(|_| ()),
        )))));

        self
    }

    /// Enables serving hidden files and directories, allowing a leading dots in url fragments.
    pub fn use_hidden_files(mut self) -> Self {
        self.hidden_files = true;
        self
    }
}

impl HttpServiceFactory for Files {
    fn register(mut self, config: &mut AppService) {
        let guards = if self.guards.is_empty() {
            None
        } else {
            let guards = std::mem::take(&mut self.guards);
            Some(
                guards
                    .into_iter()
                    .map(|guard| -> Box<dyn Guard> { Box::new(guard) })
                    .collect::<Vec<_>>(),
            )
        };

        if self.default.borrow().is_none() {
            *self.default.borrow_mut() = Some(config.default_service());
        }

        let rdef = if config.is_root() {
            ResourceDef::root_prefix(&self.path)
        } else {
            ResourceDef::prefix(&self.path)
        };

        config.register_service(rdef, guards, self, None)
    }
}

impl ServiceFactory<ServiceRequest> for Files {
    type Response = ServiceResponse;
    type Error = Error;
    type Config = ();
    type Service = FilesService;
    type InitError = ();
    type Future = LocalBoxFuture<'static, Result<Self::Service, Self::InitError>>;

    fn new_service(&self, _: ()) -> Self::Future {
        let mut srv = FilesService {
            directory: self.directory.clone(),
            index: self.index.clone(),
            show_index: self.show_index,
            redirect_to_slash: self.redirect_to_slash,
            default: None,
            renderer: self.renderer.clone(),
            mime_override: self.mime_override.clone(),
            path_filter: self.path_filter.clone(),
            file_flags: self.file_flags,
            guards: self.use_guards.clone(),
            hidden_files: self.hidden_files,
        };

        if let Some(ref default) = *self.default.borrow() {
            let fut = default.new_service(());
            Box::pin(async {
                match fut.await {
                    Ok(default) => {
                        srv.default = Some(default);
                        Ok(srv)
                    }
                    Err(_) => Err(()),
                }
            })
        } else {
            Box::pin(ok(srv))
        }
    }
}
