//! Example showing response body (chunked) stream erroring.
//!
//! Test using `nc` or `curl`.
//! ```sh
//! $ curl -vN 127.0.0.1:8080
//! $ echo 'GET / HTTP/1.1\n\n' | nc 127.0.0.1 8080
//! ```

use std::{io, time::Duration};

use actix_http::{body::BodyStream, Error, HttpService, Response};
use actix_server::Server;
use async_stream::stream;
use bytes::Bytes;

#[actix_rt::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    Server::build()
        .bind("streaming-error", ("127.0.0.1", 8080), || {
            HttpService::build()
                .finish(|req| async move {
                    log::info!("{:?}", req);
                    let res = Response::ok();

                    Ok::<_, ()>(res.set_body(BodyStream::new(stream! {
                        yield Ok(Bytes::from("123"));
                        yield Ok(Bytes::from("456"));

                        actix_rt::time::sleep(Duration::from_millis(1000)).await;

                        yield Err(Error::from(()));
                    })))
                })
                .tcp()
        })?
        .run()
        .await
}