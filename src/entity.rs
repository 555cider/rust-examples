use std::{
    error::Error,
    fmt::{Display, Formatter},
    pin::Pin,
    task::{Context, Poll},
};

#[derive(Debug)]
pub enum GatewayBody {
    Empty,
    Incoming(hyper::body::Incoming),
}

impl hyper::body::Body for GatewayBody {
    type Data = hyper::body::Bytes;
    type Error = GatewayError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Self::Data>, Self::Error>>> {
        match &mut *self.get_mut() {
            Self::Empty => Poll::Ready(None),
            Self::Incoming(incoming) => match Pin::new(incoming).poll_frame(cx) {
                Poll::Ready(Some(Ok(frame))) => Poll::Ready(Some(Ok(frame))),
                Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(GatewayError::from(err)))),
                Poll::Ready(None) => Poll::Ready(None),
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

#[derive(Debug)]
pub enum GatewayError {
    Hyper(hyper::Error),
    Io(std::io::Error),
    Http(hyper::http::Error),
    RouteNotFound(String),
}

impl Display for GatewayError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hyper(err) => write!(f, "hyper error: {}", err),
            Self::Io(err) => write!(f, "io error: {}", err),
            Self::Http(err) => write!(f, "http error: {}", err),
            Self::RouteNotFound(path) => write!(f, "route not found: {}", path),
        }
    }
}

impl Error for GatewayError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Hyper(err) => Some(err),
            Self::Io(err) => Some(err),
            Self::Http(err) => Some(err),
            Self::RouteNotFound(_) => None,
        }
    }
}

impl From<hyper::Error> for GatewayError {
    fn from(value: hyper::Error) -> Self {
        Self::Hyper(value)
    }
}

impl From<std::io::Error> for GatewayError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<hyper::http::Error> for GatewayError {
    fn from(value: hyper::http::Error) -> Self {
        Self::Http(value)
    }
}

pub fn get_gateway_response(status_code: hyper::http::StatusCode) -> hyper::Response<GatewayBody> {
    let response = hyper::Response::builder()
        .status(status_code)
        .body(GatewayBody::Empty)
        .expect("Failed to build gateway response");
    log::info!("response = {:?}", response);
    response
}
