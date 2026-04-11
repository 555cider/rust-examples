use crate::{config, connector, entity};
use hyper::{body::Incoming, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct RoutedRequest {
    pub route: config::Route,
    pub request: Request<Incoming>,
}

pub async fn send(
    routed: RoutedRequest,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError> {
    let protocol = routed.route.protocol.clone();
    let stream = connector::connect(&routed.route).await?;

    match stream {
        connector::UpstreamStream::Plain(stream) => {
            send_on_io(TokioIo::new(stream), protocol, routed.request).await
        }
        connector::UpstreamStream::Tls(stream) => {
            send_on_io(TokioIo::new(stream), protocol, routed.request).await
        }
    }
}

async fn send_on_io<T>(
    route_io: TokioIo<T>,
    protocol: config::Protocol,
    request: Request<Incoming>,
) -> Result<Response<entity::GatewayBody>, entity::GatewayError>
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    match protocol {
        config::Protocol::Http1 => {
            let (mut sender, conn) = hyper::client::conn::http1::Builder::new()
                .handshake::<_, Incoming>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/1 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(request).await?;
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
        config::Protocol::H2c => {
            let (mut sender, conn) = hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake::<_, Incoming>(route_io)
                .await?;

            tokio::spawn(async move {
                if let Err(err) = conn.await {
                    log::error!("HTTP/2 upstream connection failed: {:?}", err);
                }
            });

            let response = sender.send_request(request).await?;
            let (parts, body) = response.into_parts();
            Ok(Response::from_parts(
                parts,
                entity::GatewayBody::Incoming(body),
            ))
        }
    }
}
