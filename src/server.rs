use std::{
    any::Any,
    cmp,
    error::Error as StdError,
    fmt, io,
    marker::PhantomData,
    net,
    sync::{Arc, Mutex},
};

use actix_http::{
    body::MessageBody, CloneableExtensions, HttpService, KeepAlive, Request, Response,
};
use actix_server::{Server, ServerBuilder};
use actix_service::{
    map_config, IntoServiceFactory, Service, ServiceFactory, ServiceFactoryExt as _,
};

#[cfg(feature = "openssl")]
use actix_tls::accept::openssl::{AlpnError, SslAcceptor, SslAcceptorBuilder};
#[cfg(feature = "rustls")]
use actix_tls::accept::rustls::ServerConfig as RustlsServerConfig;

use crate::{config::AppConfig, Error};

struct Socket {
    scheme: &'static str,
    addr: net::SocketAddr,
}

struct Config {
    host: Option<String>,
    keep_alive: KeepAlive,
    client_timeout: u64,
    client_shutdown: u64,
}

/// An HTTP Server.
///
/// Create new HTTP server with application factory.
///
/// ```no_run
/// use actix_web::{web, App, HttpResponse, HttpServer};
///
/// #[actix_rt::main]
/// async fn main() -> std::io::Result<()> {
///     HttpServer::new(
///         || App::new()
///             .service(web::resource("/").to(|| HttpResponse::Ok())))
///         .bind("127.0.0.1:59090")?
///         .run()
///         .await
/// }
/// ```
pub struct HttpServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig>,
    S::Error: Into<Error>,
    S::InitError: fmt::Debug,
    S::Response: Into<Response<B>>,
    B: MessageBody,
{
    pub(super) factory: F,
    config: Arc<Mutex<Config>>,
    backlog: u32,
    sockets: Vec<Socket>,
    builder: ServerBuilder,
    on_connect_fn: Option<Arc<dyn Fn(&dyn Any, &mut CloneableExtensions) + Send + Sync>>,
    _phantom: PhantomData<(S, B)>,
}

impl<F, I, S, B> HttpServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,

    S: ServiceFactory<Request, Config = AppConfig> + 'static,
    // S::Future: 'static,
    S::Error: Into<Error> + 'static,
    S::InitError: fmt::Debug,
    S::Response: Into<Response<B>> + 'static,
    <S::Service as Service<Request>>::Future: 'static,
    S::Service: 'static,
    // S::Service: 'static,
    B: MessageBody + 'static,
    B::Error: Into<Box<dyn StdError>>,
{
    /// Create new HTTP server with application factory
    pub fn new(factory: F) -> Self {
        HttpServer {
            factory,
            config: Arc::new(Mutex::new(Config {
                host: None,
                keep_alive: KeepAlive::Timeout(5),
                client_timeout: 5000,
                client_shutdown: 5000,
            })),
            backlog: 1024,
            sockets: Vec::new(),
            builder: ServerBuilder::default(),
            on_connect_fn: None,
            _phantom: PhantomData,
        }
    }

    /// Sets function that will be called once before each connection is handled.
    /// It will receive a `&std::any::Any`, which contains underlying connection type and a
    /// [CloneableExtensions] container so that request-local data can be passed to middleware
    /// and handlers.
    ///
    /// # Connection Types
    /// - `actix_web::rt::net::TcpStream` when no TLS layer is used.
    /// - `actix_tls::rustls::TlsStream<actix_web::rt::net::TcpStream>` when using rustls.
    /// - `actix_tls::openssl::SslStream<actix_web::rt::net::TcpStream>` when using openssl.
    ///
    /// See `on_connect` example for additional details.
    pub fn on_connect<CB>(self, f: CB) -> HttpServer<F, I, S, B>
    where
        CB: Fn(&dyn Any, &mut CloneableExtensions) + Send + Sync + 'static,
    {
        HttpServer {
            factory: self.factory,
            config: self.config,
            backlog: self.backlog,
            sockets: self.sockets,
            builder: self.builder,
            on_connect_fn: Some(Arc::new(f)),
            _phantom: PhantomData,
        }
    }

    /// Set number of workers to start.
    ///
    /// By default, server uses number of available logical CPU as thread count.
    pub fn workers(mut self, num: usize) -> Self {
        self.builder = self.builder.workers(num);
        self
    }

    /// Set the maximum number of pending connections.
    ///
    /// This refers to the number of clients that can be waiting to be served.
    /// Exceeding this number results in the client getting an error when
    /// attempting to connect. It should only affect servers under significant
    /// load.
    ///
    /// Generally set in the 64-2048 range. Default value is 2048.
    ///
    /// This method should be called before `bind()` method call.
    pub fn backlog(mut self, backlog: u32) -> Self {
        self.backlog = backlog;
        self.builder = self.builder.backlog(backlog);
        self
    }

    /// Sets the maximum per-worker number of concurrent connections.
    ///
    /// All socket listeners will stop accepting connections when this limit is reached for
    /// each worker.
    ///
    /// By default max connections is set to a 25k.
    pub fn max_connections(mut self, num: usize) -> Self {
        self.builder = self.builder.maxconn(num);
        self
    }

    /// Sets the maximum per-worker concurrent connection establish process.
    ///
    /// All listeners will stop accepting connections when this limit is reached. It can be used to
    /// limit the global TLS CPU usage.
    ///
    /// By default max connections is set to a 256.
    #[allow(unused_variables)]
    pub fn max_connection_rate(self, num: usize) -> Self {
        #[cfg(any(feature = "rustls", feature = "openssl"))]
        actix_tls::accept::max_concurrent_tls_connect(num);
        self
    }

    /// Set max number of threads for each worker's blocking task thread pool.
    ///
    /// One thread pool is set up **per worker**; not shared across workers.
    ///
    /// By default set to 512 / workers.
    pub fn worker_max_blocking_threads(mut self, num: usize) -> Self {
        self.builder = self.builder.worker_max_blocking_threads(num);
        self
    }

    /// Set server keep-alive setting.
    ///
    /// By default keep alive is set to a 5 seconds.
    pub fn keep_alive<T: Into<KeepAlive>>(self, val: T) -> Self {
        self.config.lock().unwrap().keep_alive = val.into();
        self
    }

    /// Set server client timeout in milliseconds for first request.
    ///
    /// Defines a timeout for reading client request header. If a client does not transmit
    /// the entire set headers within this time, the request is terminated with
    /// the 408 (Request Time-out) error.
    ///
    /// To disable timeout set value to 0.
    ///
    /// By default client timeout is set to 5000 milliseconds.
    pub fn client_timeout(self, val: u64) -> Self {
        self.config.lock().unwrap().client_timeout = val;
        self
    }

    /// Set server connection shutdown timeout in milliseconds.
    ///
    /// Defines a timeout for shutdown connection. If a shutdown procedure does not complete
    /// within this time, the request is dropped.
    ///
    /// To disable timeout set value to 0.
    ///
    /// By default client timeout is set to 5000 milliseconds.
    pub fn client_shutdown(self, val: u64) -> Self {
        self.config.lock().unwrap().client_shutdown = val;
        self
    }

    /// Set server host name.
    ///
    /// Host name is used by application router as a hostname for url generation.
    /// Check [ConnectionInfo](super::dev::ConnectionInfo::host())
    /// documentation for more information.
    ///
    /// By default host name is set to a "localhost" value.
    pub fn server_hostname<T: AsRef<str>>(self, val: T) -> Self {
        self.config.lock().unwrap().host = Some(val.as_ref().to_owned());
        self
    }

    /// Stop actix system.
    pub fn system_exit(mut self) -> Self {
        self.builder = self.builder.system_exit();
        self
    }

    /// Disable signal handling
    pub fn disable_signals(mut self) -> Self {
        self.builder = self.builder.disable_signals();
        self
    }

    /// Timeout for graceful workers shutdown.
    ///
    /// After receiving a stop signal, workers have this much time to finish
    /// serving requests. Workers still alive after the timeout are force
    /// dropped.
    ///
    /// By default shutdown timeout sets to 30 seconds.
    pub fn shutdown_timeout(mut self, sec: u64) -> Self {
        self.builder = self.builder.shutdown_timeout(sec);
        self
    }

    /// Get addresses of bound sockets.
    pub fn addrs(&self) -> Vec<net::SocketAddr> {
        self.sockets.iter().map(|s| s.addr).collect()
    }

    /// Get addresses of bound sockets and the scheme for it.
    ///
    /// This is useful when the server is bound from different sources
    /// with some sockets listening on HTTP and some listening on HTTPS
    /// and the user should be presented with an enumeration of which
    /// socket requires which protocol.
    pub fn addrs_with_scheme(&self) -> Vec<(net::SocketAddr, &str)> {
        self.sockets.iter().map(|s| (s.addr, s.scheme)).collect()
    }

    /// Use listener for accepting incoming connection requests
    ///
    /// HttpServer does not change any configuration for TcpListener,
    /// it needs to be configured before passing it to listen() method.
    pub fn listen(mut self, lst: net::TcpListener) -> io::Result<Self> {
        let cfg = self.config.clone();
        let factory = self.factory.clone();
        let addr = lst.local_addr().unwrap();
        self.sockets.push(Socket {
            addr,
            scheme: "http",
        });
        let on_connect_fn = self.on_connect_fn.clone();

        self.builder =
            self.builder
                .listen(format!("actix-web-service-{}", addr), lst, move || {
                    let c = cfg.lock().unwrap();
                    let host = c.host.clone().unwrap_or_else(|| format!("{}", addr));

                    let mut svc = HttpService::build()
                        .keep_alive(c.keep_alive)
                        .client_timeout(c.client_timeout)
                        .client_disconnect(c.client_shutdown)
                        .local_addr(addr);

                    if let Some(handler) = on_connect_fn.clone() {
                        svc = svc.on_connect_ext(move |io: &_, ext: _| {
                            (handler)(io as &dyn Any, ext)
                        })
                    };

                    let fac = factory()
                        .into_factory()
                        .map_err(|err| err.into().error_response());

                    svc.finish(map_config(fac, move |_| {
                        AppConfig::new(false, host.clone(), addr)
                    }))
                    .tcp()
                })?;
        Ok(self)
    }

    #[cfg(feature = "openssl")]
    /// Use listener for accepting incoming tls connection requests
    ///
    /// This method sets alpn protocols to "h2" and "http/1.1"
    pub fn listen_openssl(
        self,
        lst: net::TcpListener,
        builder: SslAcceptorBuilder,
    ) -> io::Result<Self> {
        self.listen_ssl_inner(lst, openssl_acceptor(builder)?)
    }

    #[cfg(feature = "openssl")]
    fn listen_ssl_inner(
        mut self,
        lst: net::TcpListener,
        acceptor: SslAcceptor,
    ) -> io::Result<Self> {
        let factory = self.factory.clone();
        let cfg = self.config.clone();
        let addr = lst.local_addr().unwrap();
        self.sockets.push(Socket {
            addr,
            scheme: "https",
        });

        let on_connect_fn = self.on_connect_fn.clone();

        self.builder =
            self.builder
                .listen(format!("actix-web-service-{}", addr), lst, move || {
                    let c = cfg.lock().unwrap();
                    let host = c.host.clone().unwrap_or_else(|| format!("{}", addr));

                    let svc = HttpService::build()
                        .keep_alive(c.keep_alive)
                        .client_timeout(c.client_timeout)
                        .client_disconnect(c.client_shutdown)
                        .local_addr(addr);

                    let svc = if let Some(handler) = on_connect_fn.clone() {
                        svc.on_connect_ext(move |io: &_, ext: _| {
                            (&*handler)(io as &dyn Any, ext)
                        })
                    } else {
                        svc
                    };

                    let fac = factory()
                        .into_factory()
                        .map_err(|err| err.into().error_response());

                    svc.finish(map_config(fac, move |_| {
                        AppConfig::new(true, host.clone(), addr)
                    }))
                    .openssl(acceptor.clone())
                })?;

        Ok(self)
    }

    #[cfg(feature = "rustls")]
    /// Use listener for accepting incoming tls connection requests
    ///
    /// This method prepends alpn protocols "h2" and "http/1.1" to configured ones
    pub fn listen_rustls(
        self,
        lst: net::TcpListener,
        config: RustlsServerConfig,
    ) -> io::Result<Self> {
        self.listen_rustls_inner(lst, config)
    }

    #[cfg(feature = "rustls")]
    fn listen_rustls_inner(
        mut self,
        lst: net::TcpListener,
        config: RustlsServerConfig,
    ) -> io::Result<Self> {
        let factory = self.factory.clone();
        let cfg = self.config.clone();
        let addr = lst.local_addr().unwrap();
        self.sockets.push(Socket {
            addr,
            scheme: "https",
        });

        let on_connect_fn = self.on_connect_fn.clone();

        self.builder =
            self.builder
                .listen(format!("actix-web-service-{}", addr), lst, move || {
                    let c = cfg.lock().unwrap();
                    let host = c.host.clone().unwrap_or_else(|| format!("{}", addr));

                    let svc = HttpService::build()
                        .keep_alive(c.keep_alive)
                        .client_timeout(c.client_timeout)
                        .client_disconnect(c.client_shutdown);

                    let svc = if let Some(handler) = on_connect_fn.clone() {
                        svc.on_connect_ext(move |io: &_, ext: _| (handler)(io as &dyn Any, ext))
                    } else {
                        svc
                    };

                    let fac = factory()
                        .into_factory()
                        .map_err(|err| err.into().error_response());

                    svc.finish(map_config(fac, move |_| {
                        AppConfig::new(true, host.clone(), addr)
                    }))
                    .rustls(config.clone())
                })?;

        Ok(self)
    }

    /// The socket address to bind
    ///
    /// To bind multiple addresses this method can be called multiple times.
    pub fn bind<A: net::ToSocketAddrs>(mut self, addr: A) -> io::Result<Self> {
        let sockets = self.bind2(addr)?;

        for lst in sockets {
            self = self.listen(lst)?;
        }

        Ok(self)
    }

    fn bind2<A: net::ToSocketAddrs>(&self, addr: A) -> io::Result<Vec<net::TcpListener>> {
        let mut err = None;
        let mut success = false;
        let mut sockets = Vec::new();

        for addr in addr.to_socket_addrs()? {
            match create_tcp_listener(addr, self.backlog) {
                Ok(lst) => {
                    success = true;
                    sockets.push(lst);
                }
                Err(e) => err = Some(e),
            }
        }

        if success {
            Ok(sockets)
        } else if let Some(e) = err.take() {
            Err(e)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Can not bind to address.",
            ))
        }
    }

    #[cfg(feature = "openssl")]
    /// Start listening for incoming tls connections.
    ///
    /// This method sets alpn protocols to "h2" and "http/1.1"
    pub fn bind_openssl<A>(mut self, addr: A, builder: SslAcceptorBuilder) -> io::Result<Self>
    where
        A: net::ToSocketAddrs,
    {
        let sockets = self.bind2(addr)?;
        let acceptor = openssl_acceptor(builder)?;

        for lst in sockets {
            self = self.listen_ssl_inner(lst, acceptor.clone())?;
        }

        Ok(self)
    }

    #[cfg(feature = "rustls")]
    /// Start listening for incoming tls connections.
    ///
    /// This method prepends alpn protocols "h2" and "http/1.1" to configured ones
    pub fn bind_rustls<A: net::ToSocketAddrs>(
        mut self,
        addr: A,
        config: RustlsServerConfig,
    ) -> io::Result<Self> {
        let sockets = self.bind2(addr)?;
        for lst in sockets {
            self = self.listen_rustls_inner(lst, config.clone())?;
        }
        Ok(self)
    }

    #[cfg(unix)]
    /// Start listening for unix domain (UDS) connections on existing listener.
    pub fn listen_uds(mut self, lst: std::os::unix::net::UnixListener) -> io::Result<Self> {
        use actix_http::Protocol;
        use actix_rt::net::UnixStream;
        use actix_service::{fn_service, ServiceFactoryExt as _};

        let cfg = self.config.clone();
        let factory = self.factory.clone();
        let socket_addr =
            net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 8080);
        self.sockets.push(Socket {
            scheme: "http",
            addr: socket_addr,
        });

        let addr = lst.local_addr()?;
        let name = format!("actix-web-service-{:?}", addr);
        let on_connect_fn = self.on_connect_fn.clone();

        self.builder = self.builder.listen_uds(name, lst, move || {
            let c = cfg.lock().unwrap();
            let config = AppConfig::new(
                false,
                c.host.clone().unwrap_or_else(|| format!("{}", socket_addr)),
                socket_addr,
            );

            fn_service(|io: UnixStream| async { Ok((io, Protocol::Http1, None)) }).and_then({
                let mut svc = HttpService::build()
                    .keep_alive(c.keep_alive)
                    .client_timeout(c.client_timeout)
                    .client_disconnect(c.client_shutdown);

                if let Some(handler) = on_connect_fn.clone() {
                    svc = svc
                        .on_connect_ext(move |io: &_, ext: _| (&*handler)(io as &dyn Any, ext));
                }

                let fac = factory()
                    .into_factory()
                    .map_err(|err| err.into().error_response());

                svc.finish(map_config(fac, move |_| config.clone()))
            })
        })?;
        Ok(self)
    }

    /// Start listening for incoming unix domain connections.
    #[cfg(unix)]
    pub fn bind_uds<A>(mut self, addr: A) -> io::Result<Self>
    where
        A: AsRef<std::path::Path>,
    {
        use actix_http::Protocol;
        use actix_rt::net::UnixStream;
        use actix_service::{fn_service, ServiceFactoryExt as _};

        let cfg = self.config.clone();
        let factory = self.factory.clone();
        let socket_addr =
            net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(127, 0, 0, 1)), 8080);

        self.sockets.push(Socket {
            scheme: "http",
            addr: socket_addr,
        });

        self.builder = self.builder.bind_uds(
            format!("actix-web-service-{:?}", addr.as_ref()),
            addr,
            move || {
                let c = cfg.lock().unwrap();
                let config = AppConfig::new(
                    false,
                    c.host.clone().unwrap_or_else(|| format!("{}", socket_addr)),
                    socket_addr,
                );

                let fac = factory()
                    .into_factory()
                    .map_err(|err| err.into().error_response());

                fn_service(|io: UnixStream| async { Ok((io, Protocol::Http1, None)) }).and_then(
                    HttpService::build()
                        .keep_alive(c.keep_alive)
                        .client_timeout(c.client_timeout)
                        .client_disconnect(c.client_shutdown)
                        .finish(map_config(fac, move |_| config.clone())),
                )
            },
        )?;

        Ok(self)
    }
}

impl<F, I, S, B> HttpServer<F, I, S, B>
where
    F: Fn() -> I + Send + Clone + 'static,
    I: IntoServiceFactory<S, Request>,
    S: ServiceFactory<Request, Config = AppConfig>,
    S::Error: Into<Error>,
    S::InitError: fmt::Debug,
    S::Response: Into<Response<B>>,
    S::Service: 'static,
    B: MessageBody,
{
    /// Start listening for incoming connections.
    ///
    /// This method starts number of HTTP workers in separate threads.
    /// For each address this method starts separate thread which does
    /// `accept()` in a loop.
    ///
    /// This methods panics if no socket address can be bound or an `Actix` system is not yet
    /// configured.
    ///
    /// ```no_run
    /// use std::io;
    /// use actix_web::{web, App, HttpResponse, HttpServer};
    ///
    /// #[actix_rt::main]
    /// async fn main() -> io::Result<()> {
    ///     HttpServer::new(|| App::new().service(web::resource("/").to(|| HttpResponse::Ok())))
    ///         .bind("127.0.0.1:0")?
    ///         .run()
    ///         .await
    /// }
    /// ```
    pub fn run(self) -> Server {
        self.builder.run()
    }
}

fn create_tcp_listener(addr: net::SocketAddr, backlog: u32) -> io::Result<net::TcpListener> {
    use socket2::{Domain, Protocol, Socket, Type};
    let domain = Domain::for_address(addr);
    let socket = Socket::new(domain, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    // clamp backlog to max u32 that fits in i32 range
    let backlog = cmp::min(backlog, i32::MAX as u32) as i32;
    socket.listen(backlog)?;
    Ok(net::TcpListener::from(socket))
}

#[cfg(feature = "openssl")]
/// Configure `SslAcceptorBuilder` with custom server flags.
fn openssl_acceptor(mut builder: SslAcceptorBuilder) -> io::Result<SslAcceptor> {
    builder.set_alpn_select_callback(|_, protocols| {
        const H2: &[u8] = b"\x02h2";
        const H11: &[u8] = b"\x08http/1.1";

        if protocols.windows(3).any(|window| window == H2) {
            Ok(b"h2")
        } else if protocols.windows(9).any(|window| window == H11) {
            Ok(b"http/1.1")
        } else {
            Err(AlpnError::NOACK)
        }
    });

    builder.set_alpn_protos(b"\x08http/1.1\x02h2")?;

    Ok(builder.build())
}
