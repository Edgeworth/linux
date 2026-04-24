// SPDX-License-Identifier: GPL-2.0 OR MIT

//! TODO docs.

use core::{
    marker::PhantomData,
    ptr::{
        self,
        NonNull, //
    },
};

use crate::{
    bindings,
    drm::{
        self,
        gem::ttm::{
            BufferObjectForDriver,
            DriverTTM,
            InitRef,
            StageAlloc,
            TtForDriver, //
        },
    },
    error::to_result,
    prelude::*,
    types::Opaque, //
};

// Each TTM device is associated with one TTM driver implementation.
/// TODO: docs.
#[repr(C)] // get rid of this if we never use container_of.
#[pin_data(PinnedDrop)]
pub struct Device<T: DriverTTM> {
    #[pin]
    obj: Opaque<bindings::ttm_device>,
    _marker: PhantomData<T>,
}

// SAFETY: TODO
unsafe impl<T: DriverTTM> Send for Device<T> {}

// SAFETY: TODO
unsafe impl<T: DriverTTM> Sync for Device<T> {}

impl<T: DriverTTM> Device<T> {
    const VTABLE: bindings::ttm_device_funcs = bindings::ttm_device_funcs {
        ttm_tt_create: Some(Self::tt_create_callback), // Required, fallible, must unwind.
        ttm_tt_populate: None,                         // Optional, fallible, must unwind.
        ttm_tt_unpopulate: None,                       // Optional, infallible.
        ttm_tt_destroy: Some(Self::tt_destroy_callback), // Required, infallible.
        eviction_valuable: None,                       // Required, infallible.
        evict_flags: None,                             // Required, infallible.
        move_: Some(Self::move_callback),              // Required.
        delete_mem_notify: None,                       // Optional, infallible.
        swap_notify: None,                             // Optional, infallible.
        io_mem_reserve: None,                          // Optional, fallible, must unwind.
        io_mem_free: None,                             // Optional, infallible.
        io_mem_pfn: None,                              // Optional, infallible.
        access_memory: None,                           // Optional, fallible.
        release_notify: None,                          // Optional, infallible.
    };

    /// TODO: docs.
    /// # Safety
    ///   Must ensure drm::Device outlives this (by storing inside drm::Device::Data)
    pub(super) unsafe fn new(drm: &drm::Device<T>) -> impl PinInit<Self, Error> + '_ {
        pr_warn!("ttm::Device::new");
        try_pin_init!(Self {
            obj <- Opaque::try_ffi_init(|ptr| {
                // SAFETY: `anon_inode` and `i_mapping` valid for drm devices.
                let mapping = unsafe { (*(*drm.as_raw()).anon_inode).i_mapping };

                // SAFETY: `vma_offset_manager` is valid for GEM drivers.
                let vma_manager = unsafe { (*drm.as_raw()).vma_offset_manager };

                // `ttm_device_init` performs its own cleanup if it fails.
                pr_warn!(
                    "initializing TTM device with drm {:p}, mapping {:p}, vma_manager {:p}\n",
                    drm.as_raw(),
                    mapping,
                    vma_manager
                );
                // SAFETY: TODO.
                to_result(unsafe {
                    bindings::ttm_device_init(
                        ptr,
                        &Self::VTABLE,
                        drm.as_ref().as_raw(),
                        mapping,
                        vma_manager,
                        0, // TODO: flags
                    )
                })
            }),
            _marker: PhantomData,
        })
    }

    /// TODO docs.
    ///
    /// # Safety
    /// TODO
    pub(super) unsafe fn as_raw(&self) -> *mut bindings::ttm_device {
        self.obj.get()
    }

    extern "C" fn tt_create_callback(
        raw_bo: *mut bindings::ttm_buffer_object,
        page_flags: u32,
    ) -> *mut bindings::ttm_tt {
        // TODO: Must free stuff on failure.
        // TODO: Drivers execute stuff before/after ttm_tt_init (or use ttm_sg_tt_init).
        pr_warn!("tt_create_callback called with page_flags={page_flags:#x}");

        // Note: we need to recover the type here.
        // It is BufferObject<T::BufferObject> or it could be a ghost BO (ttm_transfer_obj), in
        // which case we do common operations here.

        // SAFETY: TODO
        if unsafe { BufferObjectForDriver::<T>::is_transfer_object(raw_bo) } {
            pr_warn!("tt_create_callback called for ghost BO {:p}\n", raw_bo);
            return ptr::null_mut();
        }

        pr_warn!("tt_create_callback called for driver BO {:p}\n", raw_bo);

        // SAFETY: TODO
        // TODO: this might be during initialization, so we can't let driver code take an ARef here.
        let bo = NonNull::from(unsafe { BufferObjectForDriver::<T>::from_raw_ttm(raw_bo) });

        // SAFETY: TODO
        let bo = unsafe { InitRef::<'_, BufferObjectForDriver<T>, StageAlloc>::new(bo) };

        let Ok(tt) = TtForDriver::<T>::new(bo) else {
            pr_err!("Failed to create TTM tt\n");
            return ptr::null_mut();
        };

        let raw_tt = tt.as_raw();

        // SAFETY: TODO
        let _ = KVBox::into_raw(unsafe { Pin::into_inner_unchecked(tt) });

        raw_tt
    }

    extern "C" fn tt_destroy_callback(
        _raw_ttm: *mut bindings::ttm_device,
        raw_tt: *mut bindings::ttm_tt,
    ) {
        pr_warn!("tt_destroy_callback called for tt {:p}\n", raw_tt);
        // SAFETY: TODO
        let _ = unsafe {
            KVBox::from_raw(ptr::from_ref(TtForDriver::<T>::from_raw(raw_tt)).cast_mut())
        };
    }

    extern "C" fn move_callback(
        bo: *mut bindings::ttm_buffer_object,
        evict: bool,
        ctx: *mut bindings::ttm_operation_ctx,
        new_mem: *mut bindings::ttm_resource,
        hop: *mut bindings::ttm_place,
    ) -> ffi::c_int {
        pr_warn!(
            "move_callback called for bo {:p}, evict={evict}, ctx {:p}, new_mem {:p}, hop {:p}\n",
            bo,
            ctx,
            new_mem,
            hop
        );

        // SAFETY: TODO
        unsafe { bindings::ttm_bo_move_null(bo, new_mem) };

        // TODO: the impl.

        0
    }
}

#[pinned_drop]
impl<T: DriverTTM> PinnedDrop for Device<T> {
    fn drop(self: Pin<&mut Self>) {
        pr_warn!("ttm::Device::drop");
        // Note that this will drain the workqueue.
        // SAFETY: TODO.
        unsafe { bindings::ttm_device_fini(self.obj.get()) };
    }
}
