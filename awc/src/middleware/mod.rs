mod redirect;
mod retry;

pub use self::redirect::Redirect;
pub use self::retry::Retry;

use std::marker::PhantomData;

use actix_service::Service;

/// Trait for transform a type to another one.
/// Both the input and output type should impl [actix_service::Service] trait.
pub trait Transform<S, Req> {
    type Transform: Service<Req>;

    /// Creates and returns a new Transform component.
    fn new_transform(self, service: S) -> Self::Transform;
}

#[doc(hidden)]
/// Helper struct for constructing Nested types that would call `Transform::new_transform`
/// in a chain.
///
/// The child field would be called first and the output `Service` type is
/// passed to parent as input type.
pub struct NestTransform<T1, T2, S, Req>
where
    T1: Transform<S, Req>,
    T2: Transform<T1::Transform, Req>,
{
    child: T1,
    parent: T2,
    _service: PhantomData<(S, Req)>,
}

impl<T1, T2, S, Req> NestTransform<T1, T2, S, Req>
where
    T1: Transform<S, Req>,
    T2: Transform<T1::Transform, Req>,
{
    pub(crate) fn new(child: T1, parent: T2) -> Self {
        NestTransform {
            child,
            parent,
            _service: PhantomData,
        }
    }
}

impl<T1, T2, S, Req> Transform<S, Req> for NestTransform<T1, T2, S, Req>
where
    T1: Transform<S, Req>,
    T2: Transform<T1::Transform, Req>,
{
    type Transform = T2::Transform;

    fn new_transform(self, service: S) -> Self::Transform {
        let service = self.child.new_transform(service);
        self.parent.new_transform(service)
    }
}

/// Dummy impl for kick start `NestTransform` type in `ClientBuilder` type
impl<S, Req> Transform<S, Req> for ()
where
    S: Service<Req>,
{
    type Transform = S;

    fn new_transform(self, service: S) -> Self::Transform {
        service
    }
}
