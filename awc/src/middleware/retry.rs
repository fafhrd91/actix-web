use super::Transform;
use crate::RequestHead;
use crate::{ConnectRequest, ConnectResponse};
use actix_http::body::Body;
use actix_http::client::SendRequestError;
use actix_http::http::{HeaderMap, StatusCode};
use actix_http::RequestHeadType;
use actix_service::Service;
use futures_core::future::LocalBoxFuture;
use std::ops::Deref;
use std::rc::Rc;
use std::task::{Context, Poll};

pub struct Retry(Inner);

struct Inner {
    /// Number of retries. So each request will be tried [max_retry + 1] times
    max_retry: u8,
    policies: Vec<RetryPolicy>,
}

impl Retry {
    pub fn new(retries: u8) -> Self {
        Retry(Inner {
            max_retry: retries,
            policies: vec![],
        })
    }

    /// Allows you to add a retry policy to the [`policies`]
    /// It allows two types of policy:
    ///  - `Vec<StatusCode>` and will retry if one of them is received
    ///  - `Fn(&ResponseHead) -> bool` and will retry when this function resolves to false
    ///
    /// # example
    ///
    ///```
    ///
    /// // Creates a policy which will try each request a max of 5 times if any policies resolve to true
    /// // i.e.
    /// // if you receive a 401 or 501 status code
    /// // or
    /// // the response doesn't have a [`SOME_HEADER`] header
    /// use awc::http::{StatusCode, HeaderMap};
    /// use awc::middleware::Retry;
    ///
    /// let retry_policies = Retry::new(5)
    ///     .policy(vec![StatusCode::INTERNAL_SERVER_ERROR, StatusCode::UNAUTHORIZED])
    ///     .policy(|code: StatusCode, headers: &HeaderMap| {
    ///         return if headers.contains_key("SOME_HEADER") {
    ///             true
    ///         } else {
    ///             false
    ///         };
    ///     });
    ///
    /// // Creates awc client
    /// let client = awc::Client::builder()
    ///     .wrap(retry_policies)
    ///     .finish();
    ///```
    pub fn policy<T>(mut self, p: T) -> Self
    where
        T: IntoRetryPolicy,
    {
        self.0.policies.push(p.into_policy());
        self
    }
}

#[non_exhaustive]
pub enum RetryPolicy {
    Status(Vec<StatusCode>),
    Custom(Box<dyn Fn(StatusCode, &HeaderMap) -> bool>),
}

pub trait IntoRetryPolicy {
    fn into_policy(self) -> RetryPolicy;
}

impl<T> IntoRetryPolicy for T
where
    T: for<'a> Fn(StatusCode, &'a HeaderMap) -> bool + 'static,
{
    fn into_policy(self) -> RetryPolicy {
        RetryPolicy::Custom(Box::new(self))
    }
}

impl IntoRetryPolicy for Vec<StatusCode> {
    fn into_policy(self) -> RetryPolicy {
        RetryPolicy::Status(self)
    }
}

impl<S> Transform<S, ConnectRequest> for Retry
where
    S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError> + 'static,
{
    type Transform = RetryService<S>;

    fn new_transform(self, service: S) -> Self::Transform {
        RetryService {
            max_retry: self.0.max_retry,
            policies: self.0.policies.into_boxed_slice().into(),
            connector: service.into(),
        }
    }
}

#[derive(Clone)]
pub struct RetryService<S> {
    policies: Rc<[RetryPolicy]>,
    max_retry: u8,
    connector: Rc<S>,
}

impl<S> Service<ConnectRequest> for RetryService<S>
where
    S: Service<ConnectRequest, Response = ConnectResponse, Error = SendRequestError> + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, ctx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.connector.poll_ready(ctx)
    }

    fn call(&self, req: ConnectRequest) -> Self::Future {
        let connector = self.connector.clone();
        let policies = self.policies.clone();
        let max_retry = self.max_retry;

        Box::pin(async move {
            match req {
                ConnectRequest::Client(head, body, addr) => {
                    for _ in 0..max_retry {
                        let h = clone_request_head_type(&head);

                        let result = connector
                            .call(ConnectRequest::Client(h, body_to_retry_body(&body), addr))
                            .await;

                        if let Ok(res) = result {
                            match &res {
                                ConnectResponse::Client(ref r) => {
                                    if is_valid_response(
                                        policies.as_ref(),
                                        r.status(),
                                        r.headers(),
                                    ) {
                                        return Ok(res);
                                    }
                                }
                                ConnectResponse::Tunnel(ref head, _) => {
                                    if is_valid_response(
                                        policies.as_ref(),
                                        head.status,
                                        head.headers(),
                                    ) {
                                        return Ok(res);
                                    }
                                }
                            }
                        }
                    }

                    // Exceed max retry so just return whatever response is received
                    log::debug!("Request max retry reached");
                    connector
                        .call(ConnectRequest::Client(head, body, addr))
                        .await
                }
                ConnectRequest::Tunnel(head, addr) => {
                    for _ in 1..max_retry {
                        let h = clone_request_head(&head);

                        let result = connector.call(ConnectRequest::Tunnel(h, addr)).await;

                        if let Ok(res) = result {
                            match &res {
                                ConnectResponse::Client(r) => {
                                    if is_valid_response(&policies, r.status(), r.headers()) {
                                        return Ok(res);
                                    }
                                }
                                ConnectResponse::Tunnel(head, _) => {
                                    if is_valid_response(&policies, head.status, head.headers())
                                    {
                                        return Ok(res);
                                    }
                                }
                            }
                        }
                    }

                    // Exceed max retry so just return whatever response is received
                    log::debug!("Request max retry reached");
                    connector.call(ConnectRequest::Tunnel(head, addr)).await
                }
            }
        })
    }
}

fn body_to_retry_body(body: &Body) -> Body {
    match body {
        Body::Empty => Body::Empty,
        Body::Bytes(b) => Body::Bytes(b.clone()),
        _ => Body::None,
    }
}

#[doc(hidden)]
/// Clones [RequestHeadType] except for the extensions (not required for this middleware)
fn clone_request_head_type(head_type: &RequestHeadType) -> RequestHeadType {
    match head_type {
        RequestHeadType::Owned(h) => {
            let mut inner_head = RequestHead::default();
            inner_head.uri = h.uri.clone();
            inner_head.method = h.method.clone();
            inner_head.version = h.version;
            inner_head.peer_addr = h.peer_addr;
            inner_head.headers = h.headers.clone();

            RequestHeadType::Owned(inner_head)
        }
        RequestHeadType::Rc(h, header_map) => {
            RequestHeadType::Rc(h.clone(), header_map.clone())
        }
    }
}

#[doc(hidden)]
/// Clones [RequestHeadType] except for the extensions (not required for this middleware)
fn clone_request_head(head: &RequestHead) -> RequestHead {
    let mut new_head = RequestHead::default();
    new_head.uri = head.uri.clone();
    new_head.method = head.method.clone();
    new_head.version = head.version;
    new_head.headers = head.headers.clone();
    new_head.peer_addr = head.peer_addr;

    new_head
}

#[doc(hidden)]
/// Checks whether the response matches the policies
fn is_valid_response(
    policies: &[RetryPolicy],
    status_code: StatusCode,
    headers: &HeaderMap,
) -> bool {
    policies.iter().all(|policy| {
        match policy {
            RetryPolicy::Status(v) => {
                // is valid if:
                // - the list of status codes is empty
                // or
                // - the list doesn't contain the received status code
                v.is_empty() || !v.contains(&status_code)
            }
            RetryPolicy::Custom(func) => {
                // custom policy
                (func.deref())(status_code, headers)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use actix_web::{web, App, Error, HttpResponse};

    use super::*;
    use crate::ClientBuilder;

    #[actix_rt::test]
    async fn test_basic_policy() {
        let client = ClientBuilder::new()
            .disable_redirects()
            .wrap(Retry::new(1).policy(vec![StatusCode::INTERNAL_SERVER_ERROR]))
            .finish();

        let srv = actix_test::start(|| {
            App::new().service(web::resource("/test").route(web::to(|| async {
                Ok::<_, Error>(HttpResponse::InternalServerError().finish())
            })))
        });

        let res = client.get(srv.url("/test")).send().await.unwrap();

        assert_eq!(res.status().as_u16(), 500);
    }

    #[actix_rt::test]
    async fn test_header_policy() {
        let client = ClientBuilder::new()
            .disable_redirects()
            .wrap(
                Retry::new(2).policy(|code: StatusCode, headers: &HeaderMap| {
                    code.is_success() && headers.contains_key("SOME_HEADER")
                }),
            )
            .finish();

        let srv = actix_test::start(|| {
            App::new().service(web::resource("/test").route(web::to(|| async {
                Ok::<_, Error>(
                    HttpResponse::Ok()
                        .insert_header(("SOME_HEADER", "test"))
                        .finish(),
                )
            })))
        });

        let res = client.get(srv.url("/test")).send().await.unwrap();

        assert_eq!(res.status().as_u16(), 200);
    }

    #[actix_rt::test]
    async fn test_bad_header_policy() {
        let client = ClientBuilder::new()
            .disable_redirects()
            .wrap(
                Retry::new(3).policy(|code: StatusCode, headers: &HeaderMap| {
                    code.is_success() && headers.contains_key("WRONG_HEADER")
                }),
            )
            .finish();

        let srv = actix_test::start(|| {
            App::new().service(web::resource("/test").route(web::to(|| async {
                Ok::<_, Error>(
                    HttpResponse::Ok()
                        .insert_header(("SOME_HEADER", "test"))
                        .finish(),
                )
            })))
        });

        let res = client.get(srv.url("/test")).send().await.unwrap();

        assert_eq!(res.status().as_u16(), 200);
    }
}
