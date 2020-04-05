use core::mem::{self, ManuallyDrop};
use core::ops::{Deref, DerefMut};
use stable_deref_trait::StableDeref;

pub unsafe trait RefClass<'a>: Copy {
    /// Reference which may be reborrowed from this type, given a mutable reference.
    type RefMut: HasRefClass<'a>;

    /// Reference which may be reborrowed from this type, given a shared reference.
    type RefConst;

    /// Reborrow from this reference, given a mutable reference to it.
    unsafe fn reborrow_mut(self) -> Self::RefMut;

    /// Reborrow from this reference, given a const reference to it.
    unsafe fn reborrow_const(self) -> Self::RefConst;
}

pub unsafe trait HasRefClass<'a> {
    type Class: for<'b> RefClass<'b>;
    unsafe fn into_class(self) -> Self::Class;
}

/// Types which do not hold exclusive access to their owner.
pub unsafe trait SharedRef {}

unsafe impl<'a, T: ?Sized + 'static> HasRefClass<'a> for &'a T {
    type Class = *const T;
    unsafe fn into_class(self) -> Self::Class {
        self
    }
}
unsafe impl<'a, T: ?Sized + 'static> RefClass<'a> for *const T {
    type RefMut = &'a T;
    type RefConst = &'a T;
    unsafe fn reborrow_mut(self) -> Self::RefMut {
        &*self
    }
    unsafe fn reborrow_const(self) -> Self::RefConst {
        &*self
    }
}
unsafe impl<T> SharedRef for *const T {}

unsafe impl<'a, T: ?Sized + 'static> HasRefClass<'a> for &'a mut T {
    type Class = *mut T;
    unsafe fn into_class(self) -> Self::Class {
        self
    }
}
unsafe impl<'a, T: ?Sized + 'static> RefClass<'a> for *mut T {
    type RefMut = &'a mut T;
    type RefConst = &'a T;
    unsafe fn reborrow_mut(self) -> Self::RefMut {
        &mut *self
    }
    unsafe fn reborrow_const(self) -> Self::RefConst {
        &*self
    }
}

macro_rules! tuple_impls {
    ($($($T:ident),*;)*) => {$(
        unsafe impl<'a, $($T),*> HasRefClass<'a> for ($($T,)*)
        where
            $($T: HasRefClass<'a>,)*
        {
            type Class = ($(<$T as HasRefClass<'a>>::Class,)*);
            unsafe fn into_class(self) -> Self::Class {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                ($($T.into_class(),)*)
            }
        }
        unsafe impl<'a, $($T),*> RefClass<'a> for ($($T,)*)
        where
            $($T: RefClass<'a>,)*
        {
            type RefMut = ($(<$T as RefClass<'a>>::RefMut,)*);
            type RefConst = ($(<$T as RefClass<'a>>::RefConst,)*);

            unsafe fn reborrow_mut(self) -> Self::RefMut {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                ($($T.reborrow_mut(),)*)
            }
            unsafe fn reborrow_const(self) -> Self::RefConst {
                #[allow(non_snake_case)]
                let ($($T,)*) = self;
                ($($T.reborrow_const(),)*)
            }
        }
        unsafe impl<$($T),*> SharedRef for *mut ($($T,)*) {}
    )*};
}

tuple_impls! {
    A;
    A, B;
    A, B, C;
    A, B, C, D;
    A, B, C, D, E;
    A, B, C, D, E, F;
    A, B, C, D, E, F, G;
    A, B, C, D, E, F, G, H;
    A, B, C, D, E, F, G, H, I;
    A, B, C, D, E, F, G, H, I, J;
    A, B, C, D, E, F, G, H, I, J, K;
    A, B, C, D, E, F, G, H, I, J, K, L;
    A, B, C, D, E, F, G, H, I, J, K, L, M;
    A, B, C, D, E, F, G, H, I, J, K, L, M, N;
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O;
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P;
}

pub struct OwningRef<O, T> {
    owner: ManuallyDrop<O>,
    borrow: ManuallyDrop<T>,
}

impl<O, T: ?Sized + 'static> OwningRef<O, *mut T>
where
    O: StableDeref + DerefMut<Target = T>,
{
    /// Creates a new owning reference from a owner initialized to the direct
    /// dereference of it.
    pub fn new(mut owner: O) -> Self {
        let ptr = &mut *owner as *mut T;
        OwningRef {
            owner: ManuallyDrop::new(owner),
            borrow: ManuallyDrop::new(ptr),
        }
    }
}

impl<O, T: ?Sized + 'static> OwningRef<O, *const T>
where
    O: StableDeref + Deref<Target = T>,
{
    /// Creates a new owning reference from a owner initialized to the direct
    /// dereference of it.
    pub fn new_shared(owner: O) -> Self {
        let ptr = &*owner as *const T;
        OwningRef {
            owner: ManuallyDrop::new(owner),
            borrow: ManuallyDrop::new(ptr),
        }
    }
}

impl<O, T> OwningRef<O, T>
where
    for<'a> T: RefClass<'a>,
{
    /// Convert self into a new owning reference that points at something
    /// reachable from the previous one.
    pub fn map<F>(
        mut self,
        f: F,
    ) -> OwningRef<
        O,
        <<F as map::MapFunc<'static, <T as RefClass<'static>>::RefMut>>::Output as HasRefClass<
            'static,
        >>::Class,
    >
    where
        for<'a> F: map::MapFunc<'a, <T as RefClass<'a>>::RefMut>,
    {
        unsafe {
            // Explode `self` into member parts. `owner` is taken out of its
            // `ManuallyDrop` so that it will be dropped if a panic occurs.
            let owner = ManuallyDrop::take(&mut self.owner);
            let borrow = ManuallyDrop::take(&mut self.borrow);
            mem::forget(self);

            // Reborrow `self` to call `f` with, call it, and then re-wrap it
            // into the lifetime-free `RefClass`.
            let borrow = f.call(borrow.reborrow_mut()).into_class();

            OwningRef {
                owner: ManuallyDrop::new(owner),
                borrow: ManuallyDrop::new(borrow),
            }
        }
    }

    /// Try to convert self into a new owning reference that points at something
    /// reachable from the previous one.
    pub fn try_map<F, E>(
        mut self,
        f: F,
    ) -> Result<
        OwningRef<
            O,
            <<F as map::TryMapFunc<'static, <T as RefClass<'static>>::RefMut>>::Output as HasRefClass<
                'static,
            >>::Class,
        >,
        E,
    >
    where
        for<'a> F: map::TryMapFunc<'a, <T as RefClass<'a>>::RefMut, Error = E>,
    {
        unsafe {
            // Explode `self` into member parts. `owner` is taken out of its
            // `ManuallyDrop` so that it will be dropped if a panic occurs.
            let owner = ManuallyDrop::take(&mut self.owner);
            let borrow = ManuallyDrop::take(&mut self.borrow);
            mem::forget(self);

            // Reborrow `self` to call `f` with, call it, and then re-wrap it
            // into the lifetime-free `RefClass`.
            let borrow = f.call(borrow.reborrow_mut())?.into_class();

            Ok(OwningRef {
                owner: ManuallyDrop::new(owner),
                borrow: ManuallyDrop::new(borrow),
            })
        }
    }

    /// Extract borrowed references from this OwningRef.
    pub fn borrow<'a>(&'a self) -> <T as RefClass<'a>>::RefConst {
        unsafe { self.borrow.reborrow_const() }
    }

    /// Extract borrowed mutable references from this OwningRef.
    pub fn borrow_mut<'a>(&'a mut self) -> <T as RefClass<'a>>::RefMut {
        unsafe { self.borrow.reborrow_mut() }
    }

    /// Unsafely create a new OwningRef. This method will take an arbitrary
    /// `owner`, but requires enforcing the requirements of `StableDeref`
    /// manually.
    pub unsafe fn new_raw<F>(mut owner: O, init: F) -> Self
    where
        F: FnOnce(&mut O) -> T,
    {
        let borrow = init(&mut owner);
        OwningRef {
            owner: ManuallyDrop::new(owner),
            borrow: ManuallyDrop::new(borrow),
        }
    }
}

impl<O, T> OwningRef<O, T> {
    /// Drop the owned reference, and unwrap to the underlying owner.
    pub fn into_owner(mut self) -> O {
        unsafe {
            ManuallyDrop::drop(&mut self.borrow);
            let owner = ManuallyDrop::take(&mut self.owner);
            mem::forget(self);
            owner
        }
    }

    /// Get a reference to the underlying owner.
    pub fn as_owner(&self) -> &O
    where
        T: SharedRef,
    {
        &self.owner
    }
}

#[test]
fn compile_checks() {
    #[allow(dead_code)]
    fn testy(o: OwningRef<Box<String>, *mut String>) {
        fn my_map(x: &mut String) -> &mut str {
            &mut x[..]
        }
        o.map(my_map);
    }

    #[allow(dead_code)]
    fn try_testy(o: OwningRef<Box<String>, *mut String>) {
        fn my_try_map(x: &mut String) -> Result<&mut str, u32> {
            Ok(&mut x[..])
        }
        let _ = o.try_map(my_try_map);
    }

    // pub fn evil_try_testy(o: OwningRef<Box<String>, *mut String>) {
    //     fn my_try_map<'a>(x: &'a mut String) -> Result<&'a str, &'a str> {
    //         Ok(&mut x[..])
    //     }
    //     let _ = o.try_map(my_try_map);
    // }
}

/// Traits used for `OwningRef::[try_]map`
pub mod map {
    use super::*;

    /// A function which may be passed to `OwningRef::map`
    ///
    /// This type has some complicated trait bounds, and rust will often fail to
    /// perform type inference for it properly.
    pub unsafe trait MapFunc<'a, T>
    where
        T: HasRefClass<'a>,
    {
        type Output: HasRefClass<'a>;

        fn call(self, refs: T) -> Self::Output;
    }
    unsafe impl<'a, T, U, F> MapFunc<'a, T> for F
    where
        F: FnOnce(T) -> U,
        T: HasRefClass<'a>,
        U: HasRefClass<'a>,
    {
        type Output = U;
        fn call(self, refs: T) -> U {
            self(refs)
        }
    }

    /// A function which may be passed to `OwningRef::try_map`
    ///
    /// This type has some complicated trait bounds, and rust will often fail to
    /// perform type inference for it properly.
    pub unsafe trait TryMapFunc<'a, T>
    where
        T: HasRefClass<'a>,
    {
        type Error;
        type Output: HasRefClass<'a>;

        fn call(self, refs: T) -> Result<Self::Output, Self::Error>;
    }
    unsafe impl<'a, T, U, F, E> TryMapFunc<'a, T> for F
    where
        F: FnOnce(T) -> Result<U, E>,
        T: HasRefClass<'a>,
        U: HasRefClass<'a>,
    {
        type Error = E;
        type Output = U;
        fn call(self, refs: T) -> Result<U, E> {
            self(refs)
        }
    }
}
