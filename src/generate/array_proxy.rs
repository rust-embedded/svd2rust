/// Access an array of `COUNT` items of type `T` with the items `STRIDE` bytes
/// apart.  This is a zero-sized-type.  No objects of this type are ever
/// actually created, it is only a convenience for wrapping pointer arithmetic.
///
/// There is no safe way to produce items of this type.  Unsafe code can produce
/// references by pointer casting.  It is up to the unsafe code doing that, to
/// ensure that the memory really is backed by appropriate content.
///
/// Typically, this is used for accessing hardware registers.
#[derive(Debug)]
pub struct ArrayProxy<T, const COUNT: usize, const STRIDE: usize> {
    /// As well as providing a PhantomData, this field is non-public, and
    /// therefore ensures that code outside of this module can never create
    /// an ArrayProxy.
    _array: marker::PhantomData<T>,
}
#[allow(clippy::len_without_is_empty)]
impl<T, const C: usize, const S: usize> ArrayProxy<T, C, S> {
    /// Get a reference from an [ArrayProxy] with no bounds checking.
    pub const unsafe fn get_ref(&self, index: usize) -> &T {
        &*(self as *const Self).cast::<u8>().add(S * index).cast()
    }
    /// Get a reference from an [ArrayProxy], or return `None` if the index
    /// is out of bounds.
    pub const fn get(&self, index: usize) -> Option<&T> {
        if index < C {
            Some(unsafe { self.get_ref(index) })
        } else {
            None
        }
    }
    /// Return the number of items.
    pub const fn len(&self) -> usize {
        C
    }
}

impl<T, const C: usize, const S: usize> core::ops::Index<usize> for ArrayProxy<T, C, S> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        // Do a real array dereference for the bounds check.
        #[allow(clippy::no_effect)]
        [(); C][index];
        unsafe { self.get_ref(index) }
    }
}
