use http_body::{Body as HttpBody, Frame, SizeHint};
use std::{borrow::Cow, mem::take, pin::Pin, task::Poll};

/// Request body supported by kinetics runtime.
/// In addition to empty bodies string and binary data are supported.
#[derive(Debug, Default, Eq, PartialEq)]
pub enum Body {
    /// An empty body
    #[default]
    Empty,
    /// A body containing string data
    Text(String),
    /// A body containing binary data
    Binary(Vec<u8>),
}

impl From<lambda_http::Body> for Body {
    fn from(value: lambda_http::Body) -> Self {
        match value {
            lambda_http::Body::Empty => Body::Empty,
            lambda_http::Body::Text(chars) => Body::Text(chars),
            lambda_http::Body::Binary(bytes) => Body::Binary(bytes),
            _ => unreachable!(),
        }
    }
}

impl TryFrom<Body> for lambda_http::Body {
    type Error = eyre::Error;

    fn try_from(value: Body) -> Result<Self, Self::Error> {
        match value {
            Body::Empty => Ok(lambda_http::Body::Empty),
            Body::Text(chars) => Ok(lambda_http::Body::Text(chars)),
            Body::Binary(bytes) => Ok(lambda_http::Body::Binary(bytes)),
        }
    }
}

impl TryFrom<Body> for () {
    type Error = eyre::Error;

    fn try_from(_value: Body) -> Result<Self, Self::Error> {
        // Unit struct usually implies no payload,
        // thus we just throw the body away.
        Ok(())
    }
}

impl TryFrom<Body> for String {
    type Error = eyre::Error;

    fn try_from(value: Body) -> Result<Self, Self::Error> {
        match value {
            Body::Empty => Ok(String::new()),
            Body::Text(chars) => Ok(chars),
            Body::Binary(bytes) => Ok(String::from_utf8(bytes)?),
        }
    }
}

impl TryFrom<Body> for Vec<u8> {
    type Error = eyre::Error;

    fn try_from(value: Body) -> Result<Self, Self::Error> {
        match value {
            Body::Empty => Ok(Vec::new()),
            Body::Text(chars) => Ok(chars.into_bytes()),
            Body::Binary(bytes) => Ok(bytes),
        }
    }
}

// The remaining implementation are copied from
// https://github.com/awslabs/aws-lambda-rust-runtime/blob/main/lambda-events/src/encodings/http.rs#L96-L144
// https://github.com/awslabs/aws-lambda-rust-runtime/blob/main/lambda-events/src/encodings/http.rs#L219-L246

impl From<()> for Body {
    fn from(_: ()) -> Self {
        Body::Empty
    }
}

impl<'a> From<&'a str> for Body {
    fn from(s: &'a str) -> Self {
        Body::Text(s.into())
    }
}

impl From<String> for Body {
    fn from(b: String) -> Self {
        Body::Text(b)
    }
}

impl From<Cow<'static, str>> for Body {
    #[inline]
    fn from(cow: Cow<'static, str>) -> Body {
        match cow {
            Cow::Borrowed(b) => Body::from(b.to_owned()),
            Cow::Owned(o) => Body::from(o),
        }
    }
}

impl From<Cow<'static, [u8]>> for Body {
    #[inline]
    fn from(cow: Cow<'static, [u8]>) -> Body {
        match cow {
            Cow::Borrowed(b) => Body::from(b),
            Cow::Owned(o) => Body::from(o),
        }
    }
}

impl From<Vec<u8>> for Body {
    fn from(b: Vec<u8>) -> Self {
        Body::Binary(b)
    }
}

impl<'a> From<&'a [u8]> for Body {
    fn from(b: &'a [u8]) -> Self {
        Body::Binary(b.to_vec())
    }
}

impl HttpBody for Body {
    type Data = bytes::Bytes;
    type Error = tower::BoxError;

    fn is_end_stream(&self) -> bool {
        matches!(self, Body::Empty)
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            Body::Empty => SizeHint::default(),
            Body::Text(ref s) => SizeHint::with_exact(s.len() as u64),
            Body::Binary(ref b) => SizeHint::with_exact(b.len() as u64),
        }
    }

    fn poll_frame(
        self: Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let body = take(self.get_mut());
        Poll::Ready(match body {
            Body::Empty => None,
            Body::Text(s) => Some(Ok(Frame::data(s.into()))),
            Body::Binary(b) => Some(Ok(Frame::data(b.into()))),
        })
    }
}
