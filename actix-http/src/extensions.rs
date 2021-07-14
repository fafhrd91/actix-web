use std::{
    any::{Any, TypeId},
    fmt,
};

use ahash::AHashMap;

/// A type map for request extensions.
///
/// All entries into this map must be owned types (or static references).
#[derive(Default)]
pub struct Extensions {
    /// Use FxHasher with a std HashMap with for faster
    /// lookups on the small `TypeId` (u64 equivalent) keys.
    map: AHashMap<TypeId, Box<dyn Any>>,
}

impl Extensions {
    /// Creates an empty `Extensions`.
    #[inline]
    pub fn new() -> Extensions {
        Extensions {
            map: AHashMap::default(),
        }
    }

    /// Insert an item into the map.
    ///
    /// If an item of this type was already stored, it will be replaced and returned.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    /// assert_eq!(map.insert(""), None);
    /// assert_eq!(map.insert(1u32), None);
    /// assert_eq!(map.insert(2u32), Some(1u32));
    /// assert_eq!(*map.get::<u32>().unwrap(), 2u32);
    /// ```
    pub fn insert<T: 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(downcast_owned)
    }

    /// Check if map contains an item of a given type.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    /// assert!(!map.contains::<u32>());
    ///
    /// assert_eq!(map.insert(1u32), None);
    /// assert!(map.contains::<u32>());
    /// ```
    pub fn contains<T: 'static>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }

    /// Get a reference to an item of a given type.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    /// map.insert(1u32);
    /// assert_eq!(map.get::<u32>(), Some(&1u32));
    /// ```
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_ref())
    }

    /// Get a mutable reference to an item of a given type.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    /// map.insert(1u32);
    /// assert_eq!(map.get_mut::<u32>(), Some(&mut 1u32));
    /// ```
    pub fn get_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.map
            .get_mut(&TypeId::of::<T>())
            .and_then(|boxed| boxed.downcast_mut())
    }

    /// Remove an item from the map of a given type.
    ///
    /// If an item of this type was already stored, it will be returned.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    ///
    /// map.insert(1u32);
    /// assert_eq!(map.get::<u32>(), Some(&1u32));
    ///
    /// assert_eq!(map.remove::<u32>(), Some(1u32));
    /// assert!(!map.contains::<u32>());
    /// ```
    pub fn remove<T: 'static>(&mut self) -> Option<T> {
        self.map.remove(&TypeId::of::<T>()).and_then(downcast_owned)
    }

    /// Clear the `Extensions` of all inserted extensions.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    ///
    /// map.insert(1u32);
    /// assert!(map.contains::<u32>());
    ///
    /// map.clear();
    /// assert!(!map.contains::<u32>());
    /// ```
    #[inline]
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// Extends self with the items from another `Extensions`.
    pub fn extend(&mut self, other: Extensions) {
        self.map.extend(other.map);
    }

    /// Sets (or overrides) items from cloneable extensions map into this map.
    pub(crate) fn clone_from(&mut self, other: &CloneableExtensions) {
        for (k, val) in &other.map {
            self.map.insert(*k, (**val).clone_to_any());
        }
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions").finish()
    }
}

fn downcast_owned<T: 'static>(boxed: Box<dyn Any>) -> Option<T> {
    boxed.downcast().ok().map(|boxed| *boxed)
}

#[doc(hidden)]
pub trait CloneToAny {
    /// Cast `self` into an `Any` reference.
    #[cfg(test)]
    fn any_ref(&self) -> &dyn Any;

    /// Clone `self` to a new `Box<Any>` object.
    fn clone_to_any(&self) -> Box<dyn Any>;

    /// Clone `self` to a new `Box<CloneAny>` object.
    fn clone_to_clone_any(&self) -> Box<dyn CloneAny>;
}

impl<T: Clone + Any> CloneToAny for T {
    #[cfg(test)]
    #[inline]
    fn any_ref(&self) -> &dyn Any {
        &*self
    }

    #[inline]
    fn clone_to_any(&self) -> Box<dyn Any> {
        Box::new(self.clone())
    }

    #[inline]
    fn clone_to_clone_any(&self) -> Box<dyn CloneAny> {
        Box::new(self.clone())
    }
}

/// An [`Any`] trait with an additional [`Clone`] requirement.
pub trait CloneAny: CloneToAny + Any {}
impl<T: Any + Clone> CloneAny for T {}

impl Clone for Box<dyn CloneAny> {
    #[inline]
    fn clone(&self) -> Self {
        (**self).clone_to_clone_any()
    }
}

trait UncheckedAnyExt {
    /// # Safety
    /// Caller must ensure type `T` is true type.
    #[inline]
    unsafe fn downcast_unchecked<T: 'static>(self: Box<Self>) -> Box<T> {
        Box::from_raw(Box::into_raw(self) as *mut T)
    }
}

impl UncheckedAnyExt for dyn CloneAny {}

fn downcast_cloneable<T: 'static>(boxed: Box<dyn CloneAny>) -> T {
    // Safety:
    // Box is owned and `T` is known to be true type from map containing TypeId as key.
    *unsafe { UncheckedAnyExt::downcast_unchecked::<T>(boxed) }
}

/// A type map for `on_connect` extensions.
///
/// All entries into this map must be owned types and implement `Clone` trait.
///
/// Many requests can be processed for each connection but the `on_connect` will only be run once
/// when the connection is opened. Therefore, items added to this special map type need to be cloned
/// into the regular extensions map for each request. Most useful connection information types are
/// cloneable already but you can use reference counted wrappers if not.
#[derive(Default)]
pub struct CloneableExtensions {
    /// Use FxHasher with a std HashMap with for faster
    /// lookups on the small `TypeId` (u64 equivalent) keys.
    map: AHashMap<TypeId, Box<dyn CloneAny>>,
}

impl CloneableExtensions {
    /// Insert an item into the map.
    ///
    /// If an item of this type was already stored, it will be replaced and returned.
    ///
    /// ```
    /// # use actix_http::Extensions;
    /// let mut map = Extensions::new();
    /// assert_eq!(map.insert(""), None);
    /// assert_eq!(map.insert(1u32), None);
    /// assert_eq!(map.insert(2u32), Some(1u32));
    /// assert_eq!(*map.get::<u32>().unwrap(), 2u32);
    /// ```
    pub fn insert<T: CloneAny>(&mut self, val: T) -> Option<T> {
        self.map
            .insert(TypeId::of::<T>(), Box::new(val))
            .and_then(downcast_cloneable)
    }

    #[cfg(test)]
    fn get<T: CloneAny>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|boxed| boxed.as_ref().any_ref().downcast_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remove() {
        let mut map = Extensions::new();

        map.insert::<i8>(123);
        assert!(map.get::<i8>().is_some());

        map.remove::<i8>();
        assert!(map.get::<i8>().is_none());
    }

    #[test]
    fn test_clear() {
        let mut map = Extensions::new();

        map.insert::<i8>(8);
        map.insert::<i16>(16);
        map.insert::<i32>(32);

        assert!(map.contains::<i8>());
        assert!(map.contains::<i16>());
        assert!(map.contains::<i32>());

        map.clear();

        assert!(!map.contains::<i8>());
        assert!(!map.contains::<i16>());
        assert!(!map.contains::<i32>());

        map.insert::<i8>(10);
        assert_eq!(*map.get::<i8>().unwrap(), 10);
    }

    #[test]
    fn test_integers() {
        static A: u32 = 8;

        let mut map = Extensions::new();

        map.insert::<i8>(8);
        map.insert::<i16>(16);
        map.insert::<i32>(32);
        map.insert::<i64>(64);
        map.insert::<i128>(128);
        map.insert::<u8>(8);
        map.insert::<u16>(16);
        map.insert::<u32>(32);
        map.insert::<u64>(64);
        map.insert::<u128>(128);
        map.insert::<&'static u32>(&A);
        assert!(map.get::<i8>().is_some());
        assert!(map.get::<i16>().is_some());
        assert!(map.get::<i32>().is_some());
        assert!(map.get::<i64>().is_some());
        assert!(map.get::<i128>().is_some());
        assert!(map.get::<u8>().is_some());
        assert!(map.get::<u16>().is_some());
        assert!(map.get::<u32>().is_some());
        assert!(map.get::<u64>().is_some());
        assert!(map.get::<u128>().is_some());
        assert!(map.get::<&'static u32>().is_some());
    }

    #[test]
    fn test_composition() {
        struct Magi<T>(pub T);

        struct Madoka {
            pub god: bool,
        }

        struct Homura {
            pub attempts: usize,
        }

        struct Mami {
            pub guns: usize,
        }

        let mut map = Extensions::new();

        map.insert(Magi(Madoka { god: false }));
        map.insert(Magi(Homura { attempts: 0 }));
        map.insert(Magi(Mami { guns: 999 }));

        assert!(!map.get::<Magi<Madoka>>().unwrap().0.god);
        assert_eq!(0, map.get::<Magi<Homura>>().unwrap().0.attempts);
        assert_eq!(999, map.get::<Magi<Mami>>().unwrap().0.guns);
    }

    #[test]
    fn test_extensions() {
        #[derive(Debug, PartialEq)]
        struct MyType(i32);

        let mut extensions = Extensions::new();

        extensions.insert(5i32);
        extensions.insert(MyType(10));

        assert_eq!(extensions.get(), Some(&5i32));
        assert_eq!(extensions.get_mut(), Some(&mut 5i32));

        assert_eq!(extensions.remove::<i32>(), Some(5i32));
        assert!(extensions.get::<i32>().is_none());

        assert_eq!(extensions.get::<bool>(), None);
        assert_eq!(extensions.get(), Some(&MyType(10)));
    }

    #[test]
    fn test_extend() {
        #[derive(Debug, PartialEq)]
        struct MyType(i32);

        let mut extensions = Extensions::new();

        extensions.insert(5i32);
        extensions.insert(MyType(10));

        let mut other = Extensions::new();

        other.insert(15i32);
        other.insert(20u8);

        extensions.extend(other);

        assert_eq!(extensions.get(), Some(&15i32));
        assert_eq!(extensions.get_mut(), Some(&mut 15i32));

        assert_eq!(extensions.remove::<i32>(), Some(15i32));
        assert!(extensions.get::<i32>().is_none());

        assert_eq!(extensions.get::<bool>(), None);
        assert_eq!(extensions.get(), Some(&MyType(10)));

        assert_eq!(extensions.get(), Some(&20u8));
        assert_eq!(extensions.get_mut(), Some(&mut 20u8));
    }

    #[test]
    fn test_clone_from() {
        #[derive(Clone)]
        struct NonCopy {
            num: u8,
        }

        let mut ext = Extensions::new();
        ext.insert(2isize);

        assert_eq!(ext.get::<isize>(), Some(&2isize));

        let mut more_ext = CloneableExtensions::default();
        more_ext.insert(3isize);
        more_ext.insert(3usize);
        more_ext.insert(NonCopy { num: 8 });

        ext.clone_from(&more_ext);

        assert_eq!(ext.get::<isize>(), Some(&3isize));
        assert_eq!(ext.get::<usize>(), Some(&3usize));
        assert_eq!(more_ext.get::<isize>(), Some(&3isize));
        assert_eq!(more_ext.get::<usize>(), Some(&3usize));

        assert!(ext.get::<NonCopy>().is_some());
        assert!(more_ext.get::<NonCopy>().is_some());
    }

    #[test]
    fn boxes_not_aliased() {
        let a: Box<dyn CloneAny> = Box::new(42);
        let b = a.clone_to_clone_any();
        assert_ne!(Box::into_raw(a) as *const (), Box::into_raw(b) as *const ());

        let a: Box<dyn CloneAny> = Box::new(42);
        let b = a.clone_to_any();
        assert_ne!(Box::into_raw(a) as *const (), Box::into_raw(b) as *const ());
    }
}
