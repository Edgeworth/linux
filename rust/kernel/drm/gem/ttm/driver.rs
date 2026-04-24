// SPDX-License-Identifier: GPL-2.0 OR MIT

use core::ops::Deref;

use crate::{
    device,
    drm::{
        self,
        gem::ttm::{
            self,
            BufferObject,
            DriverBufferObject,
            DriverTt,
            Tt, //
        }, //
    },
    prelude::*, //
    sync::{
        aref::ARef,
        SetOnce, //
    },
};

pub(super) type BufferObjectForDriver<T> = BufferObject<<T as DriverTTM>::BufferObject>;
pub(super) type TtForDriver<T> = Tt<<T as DriverTTM>::Tt>;

// We need to ensure that the TTM device lifetime is outlived by the drm device.
// We can't just store an ARef to the drm device in the TTM device because if a driver
// stores the TTM device in the drm device driver specific data it creates a circular reference.
/// TODO docs.
#[pin_data]
pub struct WithTTMData<T: DriverTTM> {
    #[pin]
    inner: T::DataTTM,

    // Make sure ttm::Device is destroyed after `inner` user data.
    #[pin]
    pub(super) ttm: SetOnce<Pin<KVBox<ttm::Device<T>>>>,
}

impl<T: DriverTTM> WithTTMData<T> {
    /// TODO docs.
    fn new(data: impl PinInit<T::DataTTM, Error>) -> impl PinInit<Self, Error> {
        try_pin_init!(Self {
            inner <- data,
            ttm <- SetOnce::new(),
        })
    }

    pub(super) fn ttm_device(&self) -> &ttm::Device<T> {
        // UNWRAP: TODO.
        self.ttm.as_ref().unwrap()
    }
}

impl<T: DriverTTM> drm::Device<T> {
    /// TODO docs.
    pub fn new_with_ttm(
        dev: &device::Device,
        data: impl PinInit<T::DataTTM, Error>,
    ) -> Result<ARef<Self>> {
        let drm = drm::Device::<T>::new(dev, WithTTMData::new(data))?;
        // SAFETY: We ensure the drm device outlives the TTM device by storing it in the
        // drm device data.
        let ttm_device = KVBox::try_pin_init(unsafe { ttm::Device::<T>::new(&drm) }, GFP_KERNEL)?;
        drm.ttm.populate(ttm_device);
        Ok(drm)
    }
}

impl<T: DriverTTM> Deref for WithTTMData<T> {
    type Target = T::DataTTM;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// We need to expose a particular GEM+TTM buffer object type through the drm::Driver associated
// type, support different TTM BO kinds on the driver impl side is a common use-case but complicates
// recovering the type in callbacks - it's useful to know it's either a ghost BO or the single type
// BO for the driver.
// Note to self: The type implementing this can be made different to the type
// implementing drm::Driver if we remove the bound on DriverBufferObject. So theoretically you could
// have multiple TTM driver impls with the same drm::Driver.
/// TODO: docs.
pub trait DriverTTM:
    drm::Driver<Object = ttm::BufferObject<Self::BufferObject>, Data = WithTTMData<Self>>
    + Sized
    + Send
    + 'static
{
    /// Driver specific data.
    type DataTTM: Sync + Send;

    // Note that to represent multiple BO kinds like some drivers would want to, this should be
    // handled inside `DriverBufferObject`.
    // otherwise, it's hard to recover the type in the ttm_device_funcs callbacks.
    /// The kind of TTM+GEM object driver object.
    type BufferObject: DriverBufferObject<Driver = Self>;

    // Note: keep one kind of TT object per driver.
    /// TODO docs.
    type Tt: DriverTt<Driver = Self>;

    // Maybe some delegate functionality for the device callbacks goes in here.
}
