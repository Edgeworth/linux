// SPDX-License-Identifier: GPL-2.0 OR MIT

//! TODO docs.

use crate::{
    bindings,
    drm::gem::ttm::{
        BufferObjectForDriver,
        DriverTTM,
        InitRef,
        StageAlloc, //
    },
    error::to_result,
    prelude::*,
    types::Opaque, //
};

/// TODO: docs.
pub trait DriverTt: Sized {
    /// TODO: docs.
    type Driver: DriverTTM;

    /// TODO: docs.
    fn new<'a>(
        bo: InitRef<'a, BufferObjectForDriver<Self::Driver>, StageAlloc>,
    ) -> impl PinInit<Self, Error>;
    // TODO in here.
}

/// TODO docs.
#[repr(C)]
#[pin_data(PinnedDrop)]
pub struct Tt<T: DriverTt> {
    #[pin]
    obj: Opaque<bindings::ttm_tt>,
    #[pin]
    inner: T,
}

impl<T: DriverTt> Tt<T> {
    /// TODO docs.
    pub fn new<'a>(
        bo: InitRef<'a, BufferObjectForDriver<T::Driver>, StageAlloc>,
    ) -> Result<Pin<KVBox<Self>>> {
        let this: Pin<KVBox<Self>> = KVBox::try_pin_init(
            try_pin_init!(Self {
                obj <- Opaque::init_zeroed(),
                inner <- T::new(bo),
            }),
            GFP_KERNEL,
        )?;

        // SAFETY: The arguments are all valid via the type invariants.
        to_result(unsafe {
            bindings::ttm_tt_init(
                this.as_raw(),
                bo.as_raw_ttm(),
                0, // TODO: expose page flags
                0, // TODO: expose caching enum
                0, // TODO: expose extra pages?
            )
        })?;

        Ok(this)
    }

    pub(super) fn as_raw(&self) -> *mut bindings::ttm_tt {
        self.obj.get()
    }

    /// TODO docs.
    ///
    /// # Safety
    /// TODO
    pub(super) unsafe fn from_raw<'a>(raw_tt: *mut bindings::ttm_tt) -> &'a Self {
        // SAFETY: TODO
        unsafe { &*crate::container_of!(Opaque::cast_from(raw_tt), Self, obj) }
    }
}

#[pinned_drop]
impl<T: DriverTt> PinnedDrop for Tt<T> {
    fn drop(self: Pin<&mut Self>) {
        // SAFETY: TODO.
        unsafe { bindings::ttm_tt_fini(self.as_raw()) };
    }
}
