use std::fmt::Debug;

/// Adapts an endpoint's complete result at the HTTP runtime boundary.
///
/// Existing response types implementing [`lambda_http::IntoResponse`] pass
/// through unchanged, including their errors. To choose a different error
/// policy, return a newtype implementing this trait instead. Its implementation
/// can turn domain errors into HTTP responses without changing business code.
pub trait ApiResponse<E>: Sized {
    type Response: lambda_http::IntoResponse;
    type Error: Debug;

    fn map_result(result: Result<Self, E>) -> Result<Self::Response, Self::Error>;
}

impl<R, E> ApiResponse<E> for R
where
    R: lambda_http::IntoResponse,
    E: Debug,
{
    type Response = R;
    type Error = E;

    fn map_result(result: Result<Self, E>) -> Result<Self::Response, Self::Error> {
        result
    }
}

/// Applies the response policy selected by the endpoint's return type.
/// Used by both local and deployed endpoint entrypoints.
pub fn map_result<R, E>(result: Result<R, E>) -> Result<R::Response, R::Error>
where
    R: ApiResponse<E>,
{
    R::map_result(result)
}
