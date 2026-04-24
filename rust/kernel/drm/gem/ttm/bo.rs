// SPDX-License-Identifier: GPL-2.0 OR MIT

//! TODO docs.

use pin_init::pin_data;

use crate::{
    bindings,
    drm::{
        self,
        driver::{
            AllocImpl,
            AllocOps, //
        },
        gem::{
            self,
            ttm::{
                DriverTTM,
                InitRef,
                Place,
                StageAlloc, //
            },
            IntoGEMObject,
        }, //
    },
    error::to_result,
    impl_aref_for_gem_obj,
    prelude::*,
    sync::aref::{
        ARef,
        DeferredARef, //
    },
    types::Opaque, //
};
use core::ptr::{
    self,
    NonNull, //
};

/// TODO docs.
pub trait DriverBufferObject: gem::DriverObject<Driver: DriverTTM<BufferObject = Self>> {}

/// TODO docs.
#[repr(C)]
#[pin_data]
pub struct BufferObject<T: DriverBufferObject> {
    #[pin]
    obj: Opaque<bindings::ttm_buffer_object>,
    #[pin]
    inner: T,
    // We must hold a ref to the DRM device to ensure it outlives this buffer object. Since the TTM
    // device is stored in the DRM device's private data, this also ensures the TTM device outlives
    // this buffer object, which is also required.
    //
    // NOTE: Make sure this is destroyed last.
    //
    // NOTE: This is unsound with ttm_bo_delayed_release using just a regular ARef, because we can
    // (theoretically) free the drm::Device while inside the delayed release callback, so when we go
    // back up the stack into the ttm delayed release code, the ttm device will be freed (since we
    // store the ttm device in the drm device data). Maybe we should instead have a deferred free on
    // a workqueue.
    //
    // NOTE: This whole idea is kinda bad news and I feel like if we had a proper 'drm lifetime
    // we could do this a lot more simply.
    _drm: DeferredARef<drm::Device<T::Driver>>,
}

impl<T: DriverBufferObject> drm::private::Sealed for BufferObject<T> {}

impl_aref_for_gem_obj!(impl<T> for BufferObject<T> where T: DriverBufferObject);

// SAFETY: TTM objects are thread-safe with appropriate locking, which we enforce at the type system
// level.
unsafe impl<T: DriverBufferObject> Send for BufferObject<T> {}

// SAFETY: TTM objects are thread-safe with appropriate locking, which we enforce at the type system
// level.
unsafe impl<T: DriverBufferObject> Sync for BufferObject<T> {}

impl<T: DriverBufferObject> gem::IntoGEMObject for BufferObject<T> {
    fn as_raw(&self) -> *mut bindings::drm_gem_object {
        // SAFETY: TODO.
        unsafe { &raw mut (*self.obj.get()).base }
    }

    unsafe fn from_raw<'a>(self_ptr: *mut bindings::drm_gem_object) -> &'a Self {
        // SAFETY: TODO.
        let ttm_bo_ptr =
            unsafe { crate::container_of!(self_ptr, bindings::ttm_buffer_object, base) };
        // SAFETY: TODO.
        unsafe { Self::from_raw_ttm(ttm_bo_ptr) }
    }
}

impl<T: DriverBufferObject> BufferObject<T> {
    /// `drm_gem_object_funcs` vtable suitable for GEM TTM objects.
    const VTABLE: bindings::drm_gem_object_funcs = bindings::drm_gem_object_funcs {
        free: Some(Self::free_callback),
        open: Some(gem::open_callback::<T>),
        close: Some(gem::close_callback::<T>),
        print_info: None,
        export: None,
        pin: None,
        unpin: None,
        get_sg_table: None,
        vmap: None,
        vunmap: None,
        mmap: None,
        status: None,
        rss: None,
        vm_ops: ptr::null_mut(),
        evict: None,
    };

    /// TODO docs.
    pub fn new(
        drm: &drm::Device<T::Driver>,
        size: usize, // TODO: must be page aligned, so take size in pages.
        args: T::Args,
        places: &[Place],
    ) -> Result<ARef<Self>> {
        // The C side never tries to free this with kfree, so we can use KVBox.
        let this: Pin<KVBox<Self>> = KVBox::try_pin_init(
            try_pin_init!(Self {
                obj <- Opaque::init_zeroed(),
                _drm: DeferredARef::new(drm)?,
                inner <- T::new(drm, size, args),
            }),
            GFP_KERNEL,
        )?;

        let gem = this.as_raw();

        let mut placement = bindings::ttm_placement {
            num_placement: places.len().try_into()?,
            placement: places.as_ptr().cast(),
        };

        // SAFETY: `obj.as_raw()` is guaranteed to be valid by the initialization above.
        unsafe { (*gem).funcs = &Self::VTABLE };

        pr_warn!(
            "Initializing GEM object for BO {:p} with size {}\n",
            gem,
            size
        );
        // SAFETY: TODO
        unsafe { bindings::drm_gem_private_object_init(drm.as_raw(), gem, size) };

        // SAFETY: We never move out of `this`.
        let this = KVBox::into_raw(unsafe { Pin::into_inner_unchecked(this) });

        pr_warn!("Initializing TTM buffer object for BO {:p}\n", this);
        // If this fails, it will call [`Self::destroy_callback`] which will free `this`.
        // SAFETY: The arguments are all valid via the type invariants.
        to_result(unsafe {
            bindings::ttm_bo_init_validate(
                drm.ttm_device().as_raw(),
                (*this).as_raw_ttm(),
                BufferObjectType::Device.into(), // TODO: multiple BO types?
                &mut placement,
                0,               // TODO: expose alignment (expose page alignment)
                false,           // TODO: expose interruptible
                ptr::null_mut(), // sg_table
                ptr::null_mut(), // dma_resv
                Some(Self::destroy_callback),
            )
        })?;

        // SAFETY: We're taking over the owned refcount.
        let obj = unsafe { ARef::from_raw(NonNull::new_unchecked(this)) };

        Ok(obj)
    }

    pub(super) fn as_raw_ttm(&self) -> *mut bindings::ttm_buffer_object {
        self.obj.get()
    }

    /// TODO docs.
    ///
    /// # Safety
    /// TODO
    pub(super) unsafe fn is_transfer_object(raw_bo: *const bindings::ttm_buffer_object) -> bool {
        // SAFETY: TODO
        unsafe { (*raw_bo).base.dev.is_null() }
    }

    /// TODO docs.
    ///
    /// # Safety
    ///
    /// `raw_bo` must point at the `ttm_buffer_object` embedded in a valid [`BufferObject<T>`].
    pub(super) unsafe fn from_raw_ttm<'a>(raw_bo: *mut bindings::ttm_buffer_object) -> &'a Self {
        // SAFETY: `obj` is guaranteed to be in an `BufferObject<T>` via the safety contract of this
        // function
        unsafe { &*crate::container_of!(Opaque::cast_from(raw_bo), Self, obj) }
    }

    pub(super) extern "C" fn destroy_callback(
        raw_ttm_buffer_object: *mut bindings::ttm_buffer_object,
    ) {
        // SAFETY: TTM calls this callback with the object passed to `ttm_bo_init_validate`.
        let this = unsafe { Self::from_raw_ttm(raw_ttm_buffer_object) };

        pr_warn!(
            "ttm::BufferObject::destroy_callback called for object {:p}\n",
            this.as_raw()
        );

        // SAFETY: `this` contains a valid initialized `drm_gem_object`.
        unsafe { bindings::drm_gem_object_release(this.as_raw()) };

        // SAFETY: `Self` objects are allocated with `KVBox` in `new`, and this is the final GEM
        // release callback.
        let _ = unsafe { KVBox::from_raw(ptr::from_ref(this).cast_mut()) };
    }

    // If `ttm_bo_init_validate` fails, this will not be called.
    extern "C" fn free_callback(raw_gem_object: *mut bindings::drm_gem_object) {
        // SAFETY: DRM calls this callback with the GEM object embedded in `Self`.
        let this = unsafe { Self::from_raw(raw_gem_object) };

        pr_warn!(
            "ttm::BufferObject::free_callback calling drm_gem_object_release for object {:p}\n",
            this.as_raw()
        );

        // SAFETY: `this` contains a valid initialized `ttm_buffer_object`.
        // This will call `destroy_callback` as part of its release, which will free the Rust
        // object.
        unsafe { bindings::ttm_bo_fini(this.as_raw_ttm()) };
    }
}

impl<'a, T: DriverBufferObject> InitRef<'a, BufferObject<T>, StageAlloc> {
    pub(super) fn as_raw_ttm(&self) -> *mut bindings::ttm_buffer_object {
        // SAFETY: TODO
        unsafe { (*self.as_ptr()).as_raw_ttm() }
    }
}

// TODO:
impl<T: DriverBufferObject> AllocImpl for BufferObject<T> {
    type Driver = T::Driver;

    const ALLOC_OPS: AllocOps = AllocOps {
        gem_create_object: None,
        prime_handle_to_fd: None,
        prime_fd_to_handle: None,
        gem_prime_import: None,
        gem_prime_import_sg_table: None,
        dumb_create: None,
        dumb_map_offset: None,
    };
}

/// TODO: docs.
#[repr(u32)]
pub enum BufferObjectType {
    /// TODO docs.
    Device = bindings::ttm_bo_type_ttm_bo_type_device,
    /// TODO docs.
    Kernel = bindings::ttm_bo_type_ttm_bo_type_kernel,
    /// TODO docs.
    ScatterGather = bindings::ttm_bo_type_ttm_bo_type_sg,
}

impl From<BufferObjectType> for u32 {
    fn from(value: BufferObjectType) -> Self {
        // CAST: `BufferObjectType` is `repr(u32)` and can thus be cast losslessly.
        value as u32
    }
}
