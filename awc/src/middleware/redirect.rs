use std::{
    convert::TryFrom,
    future::Future,
    net::SocketAddr,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use actix_http::{
    body::Body,
    client::{InvalidUrl, SendRequestError},
    http::{header, Method, StatusCode, Uri},
    RequestHead, RequestHeadType,
};
use actix_service::Service;
use bytes::Bytes;
use futures_core::ready;

use super::Transform;

use crate::connect::{ConnectRequest, ConnectResponse};
use crate::ClientResponse;

pub struct Redirect {
    max_redirect_times: u8,
}

impl Default for Redirect {
    fn default() -> Self {
        Self::new()
    }
}

impl Redirect {
    pub fn new() -> Self {
        Self {
            max_redirect_times: 10,
        }
    }

    pub fn max_redirect_times(mut self, times: u8) -> Self {
        self.max_redirect_times = times;
        self
    }
}

impl<S> Transform<S, ConnectRequest> for Redirect
where
    S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError> + 'static,
{
    type Transform = RedirectService<S>;

    fn new_transform(self, service: S) -> Self::Transform {
        RedirectService {
            max_redirect_times: self.max_redirect_times,
            connector: Rc::new(service),
        }
    }
}

pub struct RedirectService<S> {
    max_redirect_times: u8,
    connector: Rc<S>,
}

impl<S> Service<ConnectRequest> for RedirectService<S>
where
    S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError> + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = RedirectServiceFuture<S>;

    actix_service::forward_ready!(connector);

    fn call(&self, req: ConnectRequest) -> Self::Future {
        match req {
            ConnectRequest::Tunnel(head, addr) => {
                let fut = self.connector.call(ConnectRequest::Tunnel(head, addr));
                RedirectServiceFuture::Tunnel { fut }
            }
            ConnectRequest::Client(head, body, addr) => {
                let connector = self.connector.clone();
                let max_redirect_times = self.max_redirect_times;

                // backup the uri and method for reuse schema and authority.
                let (uri, method, headers) = match head {
                    RequestHeadType::Owned(ref head) => {
                        (head.uri.clone(), head.method.clone(), head.headers.clone())
                    }
                    RequestHeadType::Rc(ref head, ..) => {
                        (head.uri.clone(), head.method.clone(), head.headers.clone())
                    }
                };

                let body_opt = match body {
                    Body::Bytes(ref b) => Some(b.clone()),
                    _ => None,
                };

                let fut = connector.call(ConnectRequest::Client(head, body, addr));

                RedirectServiceFuture::Client {
                    fut,
                    max_redirect_times,
                    uri: Some(uri),
                    method: Some(method),
                    headers: Some(headers),
                    body: body_opt,
                    addr,
                    connector: Some(connector),
                }
            }
        }
    }
}

pin_project_lite::pin_project! {
    #[project = RedirectServiceProj]
    pub enum RedirectServiceFuture<S>
    where
        S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError>,
        S: 'static
    {
        Tunnel { #[pin] fut: S::Future },
        Client {
            #[pin]
            fut: S::Future,
            max_redirect_times: u8,
            uri: Option<Uri>,
            method: Option<Method>,
            headers: Option<header::HeaderMap>,
            body: Option<Bytes>,
            addr: Option<SocketAddr>,
            connector: Option<Rc<S>>,
        }
    }
}

impl<S> Future for RedirectServiceFuture<S>
where
    S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError> + 'static,
{
    type Output = Result<ConnectResponse, SendRequestError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            RedirectServiceProj::Tunnel { fut } => fut.poll(cx),
            RedirectServiceProj::Client {
                fut,
                max_redirect_times,
                uri,
                method,
                headers,
                body,
                addr,
                connector,
            } => match ready!(fut.poll(cx))? {
                ConnectResponse::Client(res) => match res.head().status {
                    StatusCode::MOVED_PERMANENTLY
                    | StatusCode::FOUND
                    | StatusCode::SEE_OTHER
                    | StatusCode::TEMPORARY_REDIRECT
                    | StatusCode::PERMANENT_REDIRECT
                        if *max_redirect_times > 0 =>
                    {
                        let is_redirect = res.head().status == StatusCode::TEMPORARY_REDIRECT
                            || res.head().status == StatusCode::PERMANENT_REDIRECT;

                        let prev_uri = uri.take().unwrap();

                        // rebuild uri from the location header value.
                        let next_uri = build_next_uri(&res, &prev_uri)?;

                        // take ownership of states that could be reused
                        let addr = addr.take();
                        let connector = connector.take();

                        // reset method
                        let method = if is_redirect {
                            method.take().unwrap()
                        } else {
                            let method = method.take().unwrap();
                            match method {
                                Method::GET | Method::HEAD => method,
                                _ => Method::GET,
                            }
                        };

                        let mut body = body.take();
                        let body_new = if is_redirect {
                            // try to reuse body
                            match body {
                                Some(ref bytes) => Body::Bytes(bytes.clone()),
                                // TODO: should this be Body::Empty or Body::None.
                                _ => Body::Empty,
                            }
                        } else {
                            body = None;
                            // remove body
                            Body::None
                        };

                        let mut headers = headers.take().unwrap();

                        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);

                        // use a new request head.
                        let mut head = RequestHead::default();
                        head.uri = next_uri.clone();
                        head.method = method.clone();
                        head.headers = headers.clone();

                        let head = RequestHeadType::Owned(head);

                        let mut max_redirect_times = *max_redirect_times;
                        max_redirect_times -= 1;

                        let fut = connector
                            .as_ref()
                            .unwrap()
                            .call(ConnectRequest::Client(head, body_new, addr));

                        self.set(RedirectServiceFuture::Client {
                            fut,
                            max_redirect_times,
                            uri: Some(next_uri),
                            method: Some(method),
                            headers: Some(headers),
                            body,
                            addr,
                            connector,
                        });

                        self.poll(cx)
                    }
                    _ => Poll::Ready(Ok(ConnectResponse::Client(res))),
                },
                _ => unreachable!("ConnectRequest::Tunnel is not handled by Redirect"),
            },
        }
    }
}

fn build_next_uri(res: &ClientResponse, prev_uri: &Uri) -> Result<Uri, SendRequestError> {
    let uri = res
        .headers()
        .get(header::LOCATION)
        .map(|value| {
            // try to parse the location to a full uri
            let uri = Uri::try_from(value.as_bytes())
                .map_err(|e| SendRequestError::Url(InvalidUrl::HttpError(e.into())))?;
            if uri.scheme().is_none() || uri.authority().is_none() {
                let uri = Uri::builder()
                    .scheme(prev_uri.scheme().cloned().unwrap())
                    .authority(prev_uri.authority().cloned().unwrap())
                    .path_and_query(value.as_bytes())
                    .build()?;
                Ok::<_, SendRequestError>(uri)
            } else {
                Ok(uri)
            }
        })
        // TODO: this error type is wrong.
        .ok_or(SendRequestError::Url(InvalidUrl::MissingScheme))??;

    Ok(uri)
}

fn remove_sensitive_headers(headers: &mut header::HeaderMap, prev_uri: &Uri, next_uri: &Uri) {
    if next_uri.host() != prev_uri.host()
        || next_uri.port() != prev_uri.port()
        || next_uri.scheme() != prev_uri.scheme()
    {
        headers.remove(header::COOKIE);
        headers.remove(header::AUTHORIZATION);
        headers.remove(header::PROXY_AUTHORIZATION);
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{web, App, Error, HttpRequest, HttpResponse};

    use super::*;
    use crate::http::HeaderValue;
    use crate::ClientBuilder;
    use std::str::FromStr;

    #[actix_rt::test]
    async fn test_basic_redirect() {
        let client = ClientBuilder::new()
            .disable_redirects()
            .wrap(Redirect::new().max_redirect_times(10))
            .finish();

        let srv = actix_test::start(|| {
            App::new()
                .service(web::resource("/test").route(web::to(|| async {
                    Ok::<_, Error>(HttpResponse::BadRequest())
                })))
                .service(web::resource("/").route(web::to(|| async {
                    Ok::<_, Error>(
                        HttpResponse::Found()
                            .append_header(("location", "/test"))
                            .finish(),
                    )
                })))
        });

        let res = client.get(srv.url("/")).send().await.unwrap();

        assert_eq!(res.status().as_u16(), 400);
    }

    #[actix_rt::test]
    async fn test_redirect_limit() {
        let client = ClientBuilder::new()
            .disable_redirects()
            .wrap(Redirect::new().max_redirect_times(1))
            .connector(crate::Connector::new())
            .finish();

        let srv = actix_test::start(|| {
            App::new()
                .service(web::resource("/").route(web::to(|| async {
                    Ok::<_, Error>(
                        HttpResponse::Found()
                            .append_header(("location", "/test"))
                            .finish(),
                    )
                })))
                .service(web::resource("/test").route(web::to(|| async {
                    Ok::<_, Error>(
                        HttpResponse::Found()
                            .append_header(("location", "/test2"))
                            .finish(),
                    )
                })))
                .service(web::resource("/test2").route(web::to(|| async {
                    Ok::<_, Error>(HttpResponse::BadRequest())
                })))
        });

        let res = client.get(srv.url("/")).send().await.unwrap();

        assert_eq!(res.status().as_u16(), 302);
    }

    #[actix_rt::test]
    async fn test_redirect_status_kind_307_308() {
        let srv = actix_test::start(|| {
            async fn root() -> HttpResponse {
                HttpResponse::TemporaryRedirect()
                    .append_header(("location", "/test"))
                    .finish()
            }

            async fn test(req: HttpRequest, body: Bytes) -> HttpResponse {
                if req.method() == Method::POST && !body.is_empty() {
                    HttpResponse::Ok().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            App::new()
                .service(web::resource("/").route(web::to(root)))
                .service(web::resource("/test").route(web::to(test)))
        });

        let client = ClientBuilder::new().finish();
        let res = client.post(srv.url("/")).send_body("Hello").await.unwrap();
        assert_eq!(res.status().as_u16(), 200);
    }

    #[actix_rt::test]
    async fn test_redirect_status_kind_301_302_303() {
        let srv = actix_test::start(|| {
            async fn root() -> HttpResponse {
                HttpResponse::Found()
                    .append_header(("location", "/test"))
                    .finish()
            }

            async fn test(req: HttpRequest, body: Bytes) -> HttpResponse {
                if (req.method() == Method::GET || req.method() == Method::HEAD)
                    && body.is_empty()
                {
                    HttpResponse::Ok().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            App::new()
                .service(web::resource("/").route(web::to(root)))
                .service(web::resource("/test").route(web::to(test)))
        });

        let client = ClientBuilder::new().finish();
        let res = client.post(srv.url("/")).send_body("Hello").await.unwrap();
        assert_eq!(res.status().as_u16(), 200);

        let res = client.head(srv.url("/")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 200);
    }

    #[actix_rt::test]
    async fn test_redirect_headers() {
        let srv = actix_test::start(|| {
            async fn root(req: HttpRequest) -> HttpResponse {
                if req
                    .headers()
                    .get("custom")
                    .unwrap_or(&HeaderValue::from_str("").unwrap())
                    == "value"
                {
                    HttpResponse::Found()
                        .append_header(("location", "/test"))
                        .finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            async fn test(req: HttpRequest) -> HttpResponse {
                if req
                    .headers()
                    .get("custom")
                    .unwrap_or(&HeaderValue::from_str("").unwrap())
                    == "value"
                {
                    HttpResponse::Ok().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            App::new()
                .service(web::resource("/").route(web::to(root)))
                .service(web::resource("/test").route(web::to(test)))
        });

        let client = ClientBuilder::new()
            .header("custom", "value")
            .disable_redirects()
            .finish();
        let res = client.get(srv.url("/")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 302);

        let client = ClientBuilder::new().header("custom", "value").finish();
        let res = client.get(srv.url("/")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 200);

        let client = ClientBuilder::new().finish();
        let res = client
            .get(srv.url("/"))
            .insert_header(("custom", "value"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status().as_u16(), 200);
    }

    #[actix_rt::test]
    async fn test_redirect_cross_origin_headers() {
        // defining two services to have two different origins
        let srv2 = actix_test::start(|| {
            async fn root(req: HttpRequest) -> HttpResponse {
                if req.headers().get(header::AUTHORIZATION).is_none() {
                    HttpResponse::Ok().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            App::new().service(web::resource("/").route(web::to(root)))
        });
        let srv2_port: u16 = srv2.addr().port();

        let srv1 = actix_test::start(move || {
            async fn root(req: HttpRequest) -> HttpResponse {
                let port = *req.app_data::<u16>().unwrap();
                if req.headers().get(header::AUTHORIZATION).is_some() {
                    HttpResponse::Found()
                        .append_header((
                            "location",
                            format!("http://localhost:{}/", port).as_str(),
                        ))
                        .finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            async fn test1(req: HttpRequest) -> HttpResponse {
                if req.headers().get(header::AUTHORIZATION).is_some() {
                    HttpResponse::Found()
                        .append_header(("location", "/test2"))
                        .finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            async fn test2(req: HttpRequest) -> HttpResponse {
                if req.headers().get(header::AUTHORIZATION).is_some() {
                    HttpResponse::Ok().finish()
                } else {
                    HttpResponse::InternalServerError().finish()
                }
            }

            App::new()
                .app_data(srv2_port)
                .service(web::resource("/").route(web::to(root)))
                .service(web::resource("/test1").route(web::to(test1)))
                .service(web::resource("/test2").route(web::to(test2)))
        });

        // send a request to different origins, http://srv1/ then http://srv2/. So it should remove the header
        let client = ClientBuilder::new()
            .header(header::AUTHORIZATION, "auth_key_value")
            .finish();
        let res = client.get(srv1.url("/")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 200);

        // send a request to same origin, http://srv1/test1 then http://srv1/test2. So it should NOT remove any header
        let res = client.get(srv1.url("/test1")).send().await.unwrap();
        assert_eq!(res.status().as_u16(), 200);
    }

    #[actix_rt::test]
    async fn test_remove_sensitive_headers() {
        fn gen_headers() -> header::HeaderMap {
            let mut headers = header::HeaderMap::new();
            headers.insert(header::USER_AGENT, HeaderValue::from_str("value").unwrap());
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str("value").unwrap(),
            );
            headers.insert(
                header::PROXY_AUTHORIZATION,
                HeaderValue::from_str("value").unwrap(),
            );
            headers.insert(header::COOKIE, HeaderValue::from_str("value").unwrap());
            headers
        }

        // Same origin
        let prev_uri = Uri::from_str("https://host/path1").unwrap();
        let next_uri = Uri::from_str("https://host/path2").unwrap();
        let mut headers = gen_headers();
        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);
        assert_eq!(headers.len(), 4);

        // different schema
        let prev_uri = Uri::from_str("http://host/").unwrap();
        let next_uri = Uri::from_str("https://host/").unwrap();
        let mut headers = gen_headers();
        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);
        assert_eq!(headers.len(), 1);

        // different host
        let prev_uri = Uri::from_str("https://host1/").unwrap();
        let next_uri = Uri::from_str("https://host2/").unwrap();
        let mut headers = gen_headers();
        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);
        assert_eq!(headers.len(), 1);

        // different port
        let prev_uri = Uri::from_str("https://host:12/").unwrap();
        let next_uri = Uri::from_str("https://host:23/").unwrap();
        let mut headers = gen_headers();
        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);
        assert_eq!(headers.len(), 1);

        // different everything!
        let prev_uri = Uri::from_str("http://host1:12/path1").unwrap();
        let next_uri = Uri::from_str("https://host2:23/path2").unwrap();
        let mut headers = gen_headers();
        remove_sensitive_headers(&mut headers, &prev_uri, &next_uri);
        assert_eq!(headers.len(), 1);
    }
}
