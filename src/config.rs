use std::net::SocketAddr;
use std::rc::Rc;

use actix_http::Extensions;
use actix_router::ResourceDef;
use actix_service::{boxed, IntoServiceFactory, ServiceFactory};

use crate::data::Data;
use crate::error::Error;
use crate::guard::Guard;
use crate::resource::Resource;
use crate::rmap::ResourceMap;
use crate::route::Route;
use crate::service::{
    AppServiceFactory, HttpServiceFactory, ServiceFactoryWrapper, ServiceRequest,
    ServiceResponse,
};

type Guards = Vec<Box<dyn Guard>>;
type HttpNewService = boxed::BoxServiceFactory<(), ServiceRequest, ServiceResponse, Error, ()>;

/// Application configuration
pub struct AppService {
    config: AppConfig,
    root: bool,
    default: Rc<HttpNewService>,
    services: Vec<(
        ResourceDef,
        HttpNewService,
        Option<Guards>,
        Option<Rc<ResourceMap>>,
    )>,
}

impl AppService {
    /// Crate server settings instance.
    pub(crate) fn new(config: AppConfig, default: Rc<HttpNewService>) -> Self {
        AppService {
            config,
            default,
            root: true,
            services: Vec::new(),
        }
    }

    /// Check if root is being configured
    pub fn is_root(&self) -> bool {
        self.root
    }

    pub(crate) fn into_services(
        self,
    ) -> (
        AppConfig,
        Vec<(
            ResourceDef,
            HttpNewService,
            Option<Guards>,
            Option<Rc<ResourceMap>>,
        )>,
    ) {
        (self.config, self.services)
    }

    pub(crate) fn clone_config(&self) -> Self {
        AppService {
            config: self.config.clone(),
            default: self.default.clone(),
            services: Vec::new(),
            root: false,
        }
    }

    /// Service configuration
    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Default resource
    pub fn default_service(&self) -> Rc<HttpNewService> {
        self.default.clone()
    }

    /// Register HTTP service.
    pub fn register_service<F, S>(
        &mut self,
        rdef: ResourceDef,
        guards: Option<Vec<Box<dyn Guard>>>,
        factory: F,
        nested: Option<Rc<ResourceMap>>,
    ) where
        F: IntoServiceFactory<S, ServiceRequest>,
        S: ServiceFactory<
                ServiceRequest,
                Config = (),
                Response = ServiceResponse,
                Error = Error,
                InitError = (),
            > + 'static,
    {
        self.services
            .push((rdef, boxed::factory(factory.into_factory()), guards, nested));
    }
}

/// Application connection config
#[derive(Clone)]
pub struct AppConfig {
    secure: bool,
    host: String,
    addr: SocketAddr,
}

impl AppConfig {
    pub(crate) fn new(secure: bool, addr: SocketAddr, host: String) -> Self {
        AppConfig { secure, addr, host }
    }

    /// Server host name.
    ///
    /// Host name is used by application router as a hostname for url generation.
    /// Check [ConnectionInfo](super::dev::ConnectionInfo::host())
    /// documentation for more information.
    ///
    /// By default host name is set to a "localhost" value.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns true if connection is secure(https)
    pub fn secure(&self) -> bool {
        self.secure
    }

    /// Returns the socket address of the local half of this TCP connection
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig::new(
            false,
            "127.0.0.1:8080".parse().unwrap(),
            "localhost:8080".to_owned(),
        )
    }
}

/// Enables parts of app configuration to be declared separately from the app itself. Helpful for
/// modularizing large applications.
///
/// Merge a `ServiceConfig` into an app using [`App::configure`](crate::App::configure). Scope and
/// resources services have similar methods.
///
/// ```
/// use actix_web::{web, App, HttpResponse};
///
/// // this function could be located in different module
/// fn config(cfg: &mut web::ServiceConfig) {
///     cfg.service(web::resource("/test")
///         .route(web::get().to(|| HttpResponse::Ok()))
///         .route(web::head().to(|| HttpResponse::MethodNotAllowed()))
///     );
/// }
///
/// // merge `/test` routes from config function to App
/// App::new().configure(config);
/// ```
pub struct ServiceConfig {
    pub(crate) services: Vec<Box<dyn AppServiceFactory>>,
    pub(crate) external: Vec<ResourceDef>,
    pub(crate) app_data: Extensions,
}

impl ServiceConfig {
    pub(crate) fn new() -> Self {
        Self {
            services: Vec::new(),
            external: Vec::new(),
            app_data: Extensions::new(),
        }
    }

    /// Add shared app data item.
    ///
    /// Counterpart to [`App::data()`](crate::App::data).
    pub fn data<U: 'static>(&mut self, data: U) -> &mut Self {
        self.app_data(Data::new(data));
        self
    }

    /// Add arbitrary app data item.
    ///
    /// Counterpart to [`App::app_data()`](crate::App::app_data).
    pub fn app_data<U: 'static>(&mut self, ext: U) -> &mut Self {
        self.app_data.insert(ext);
        self
    }

    /// Run external configuration as part of the application building process
    ///
    /// Counterpart to [`App::configure()`](crate::App::configure) that allows for easy nesting.
    pub fn configure<F>(&mut self, f: F) -> &mut Self
    where
        F: FnOnce(&mut ServiceConfig),
    {
        f(self);
        self
    }

    /// Configure route for a specific path.
    ///
    /// Counterpart to [`App::route()`](crate::App::route).
    pub fn route(&mut self, path: &str, mut route: Route) -> &mut Self {
        self.service(
            Resource::new(path)
                .add_guards(route.take_guards())
                .route(route),
        )
    }

    /// Register HTTP service factory.
    ///
    /// Counterpart to [`App::service()`](crate::App::service).
    pub fn service<F>(&mut self, factory: F) -> &mut Self
    where
        F: HttpServiceFactory + 'static,
    {
        self.services
            .push(Box::new(ServiceFactoryWrapper::new(factory)));
        self
    }

    /// Register an external resource.
    ///
    /// External resources are useful for URL generation purposes only and are never considered for
    /// matching at request time. Calls to [`HttpRequest::url_for()`](crate::HttpRequest::url_for)
    /// will work as expected.
    ///
    /// Counterpart to [`App::external_resource()`](crate::App::external_resource).
    pub fn external_resource<N, U>(&mut self, name: N, url: U) -> &mut Self
    where
        N: AsRef<str>,
        U: AsRef<str>,
    {
        let mut rdef = ResourceDef::new(url.as_ref());
        *rdef.name_mut() = name.as_ref().to_string();
        self.external.push(rdef);
        self
    }
}

#[cfg(test)]
mod tests {
    use actix_service::Service;
    use bytes::Bytes;

    use super::*;
    use crate::http::{Method, StatusCode};
    use crate::test::{call_service, init_service, read_body, TestRequest};
    use crate::{web, App, HttpRequest, HttpResponse};

    #[actix_rt::test]
    async fn test_data() {
        let cfg = |cfg: &mut ServiceConfig| {
            cfg.data(10usize);
            cfg.app_data(15u8);
        };

        let srv = init_service(App::new().configure(cfg).service(web::resource("/").to(
            |_: web::Data<usize>, req: HttpRequest| {
                assert_eq!(*req.app_data::<u8>().unwrap(), 15u8);
                HttpResponse::Ok()
            },
        )))
        .await;
        let req = TestRequest::default().to_request();
        let resp = srv.call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // #[actix_rt::test]
    // async fn test_data_factory() {
    //     let cfg = |cfg: &mut ServiceConfig| {
    //         cfg.data_factory(|| {
    //             sleep(std::time::Duration::from_millis(50)).then(|_| {
    //                 println!("READY");
    //                 Ok::<_, ()>(10usize)
    //             })
    //         });
    //     };

    //     let srv =
    //         init_service(App::new().configure(cfg).service(
    //             web::resource("/").to(|_: web::Data<usize>| HttpResponse::Ok()),
    //         ));
    //     let req = TestRequest::default().to_request();
    //     let resp = srv.call(req).await.unwrap();
    //     assert_eq!(resp.status(), StatusCode::OK);

    //     let cfg2 = |cfg: &mut ServiceConfig| {
    //         cfg.data_factory(|| Ok::<_, ()>(10u32));
    //     };
    //     let srv = init_service(
    //         App::new()
    //             .service(web::resource("/").to(|_: web::Data<usize>| HttpResponse::Ok()))
    //             .configure(cfg2),
    //     );
    //     let req = TestRequest::default().to_request();
    //     let resp = srv.call(req).await.unwrap();
    //     assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    // }

    #[actix_rt::test]
    async fn test_external_resource() {
        let srv = init_service(
            App::new()
                .configure(|cfg| {
                    cfg.external_resource("youtube", "https://youtube.com/watch/{video_id}");
                })
                .route(
                    "/test",
                    web::get().to(|req: HttpRequest| {
                        HttpResponse::Ok()
                            .body(req.url_for("youtube", &["12345"]).unwrap().to_string())
                    }),
                ),
        )
        .await;
        let req = TestRequest::with_uri("/test").to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let body = read_body(resp).await;
        assert_eq!(body, Bytes::from_static(b"https://youtube.com/watch/12345"));
    }

    #[actix_rt::test]
    async fn test_service() {
        let srv = init_service(App::new().configure(|cfg| {
            cfg.service(web::resource("/test").route(web::get().to(HttpResponse::Created)))
                .route("/index.html", web::get().to(HttpResponse::Ok));
        }))
        .await;

        let req = TestRequest::with_uri("/test")
            .method(Method::GET)
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::CREATED);

        let req = TestRequest::with_uri("/index.html")
            .method(Method::GET)
            .to_request();
        let resp = call_service(&srv, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
