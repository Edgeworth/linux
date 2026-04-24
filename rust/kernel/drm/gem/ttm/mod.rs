// SPDX-License-Identifier: GPL-2.0 OR MIT

#![cfg(CONFIG_DRM_TTM)]

//! DRM TTM API
#![allow(dead_code)]

use core::{
    marker::PhantomData,
    ptr::NonNull, //
};

use crate::prelude::*;

mod bo;
pub use self::bo::*;

mod device;
pub use self::device::*;

mod placement;
pub use self::placement::*;

mod tt;
pub use self::tt::*;

mod driver;
pub use self::driver::*;

// Structure intended for partially initialised objects. Only define accessors that are valid.
/// TODO docs.
pub struct StageAlloc;

/// TODO docs.
pub struct InitRef<'a, T, Stage> {
    ptr: NonNull<T>,
    _lifetime: PhantomData<*mut &'a T>,
    _stage: PhantomData<Stage>,
}

impl<T, Stage> Copy for InitRef<'_, T, Stage> {}

impl<T, Stage> Clone for InitRef<'_, T, Stage> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, T, Stage> InitRef<'a, T, Stage> {
    /// TODO docs.
    /// # Safety
    ///  TODO
    // Consider assigning an initialization stage to a referenced object to be an unsafe operation.
    unsafe fn new(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _lifetime: PhantomData,
            _stage: PhantomData,
        }
    }

    fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }
}

#[kunit_tests(rust_drm_ttm)]
mod tests {
    use super::*;
    use crate::{
        drm::{
            self,
            gem, //
        },
        faux,
        sync::aref::ARef, //
    };
    // The bare minimum needed to create a fake drm driver for kunit

    #[pin_data]
    struct KunitData {}

    struct KunitDriver;

    const INFO: drm::DriverInfo = drm::DriverInfo {
        major: 0,
        minor: 0,
        patchlevel: 0,
        name: c"kunit",
        desc: c"Kunit",
    };

    impl DriverBufferObject for KunitObject {}

    #[vtable]
    impl drm::Driver for KunitDriver {
        type Data = WithTTMData<KunitDriver>;
        type File = KunitFile;
        type Object = BufferObject<KunitObject>;

        const INFO: drm::DriverInfo = INFO;
        const IOCTLS: &'static [drm::ioctl::DrmIoctlDescriptor] = &[];
    }

    impl DriverTTM for KunitDriver {
        type DataTTM = KunitData;
        type BufferObject = KunitObject;
        type Tt = KunitTt;
    }

    struct KunitFile;

    impl drm::file::DriverFile for KunitFile {
        type Driver = KunitDriver;

        fn open(_dev: &drm::Device<KunitDriver>) -> Result<Pin<KBox<Self>>> {
            Ok(KBox::new(Self, GFP_KERNEL)?.into())
        }
    }

    #[pin_data]
    struct KunitObject {}

    impl gem::DriverObject for KunitObject {
        type Driver = KunitDriver;
        type Args = ();

        fn new(
            _dev: &drm::Device<KunitDriver>,
            size: usize,
            _args: Self::Args,
        ) -> impl PinInit<Self, Error> {
            pr_warn!("Creating gem object of size {}\n", size);
            try_pin_init!(KunitObject {})
        }
    }

    #[pin_data]
    struct KunitTt {}

    impl DriverTt for KunitTt {
        type Driver = KunitDriver;

        fn new<'a>(
            _bo: InitRef<'a, BufferObject<KunitObject>, StageAlloc>,
        ) -> impl PinInit<Self, Error> {
            pr_warn!("Creating TTM tt\n");
            try_pin_init!(KunitTt {})
        }
    }

    fn create_drm_dev() -> Result<(faux::Registration, ARef<drm::Device<KunitDriver>>)> {
        // Create a faux DRM device so we can test gem object creation.
        let dev = faux::Registration::new(c"Kunit", None)?;
        let drm =
            drm::Device::<KunitDriver>::new_with_ttm(dev.as_ref(), try_pin_init!(KunitData {}))?;

        Ok((dev, drm))
    }

    #[test]
    fn create_ttm_device() -> Result {
        let (_dev, drm) = create_drm_dev()?;

        // Create GEM+TTM BO.
        pr_warn!("Creating GEM+TTM BO\n");
        let _bo = BufferObject::<KunitObject>::new(
            &drm,
            4096,
            (),
            &[Place::new(
                PfnRange::ALL,
                PlaceType::SYSTEM,
                PlaceFlags::empty(),
            )],
        )?;

        Ok(())
    }

    #[pin_data]
    struct GemOnlyData {}

    struct GemOnlyDriver;

    struct GemOnlyFile;

    impl drm::file::DriverFile for GemOnlyFile {
        type Driver = GemOnlyDriver;

        fn open(_dev: &drm::Device<GemOnlyDriver>) -> Result<Pin<KBox<Self>>> {
            Ok(KBox::new(Self, GFP_KERNEL)?.into())
        }
    }

    #[pin_data]
    struct GemOnlyObject {}

    impl gem::DriverObject for GemOnlyObject {
        type Driver = GemOnlyDriver;
        type Args = ();

        fn new(
            _dev: &drm::Device<GemOnlyDriver>,
            _size: usize,
            _args: Self::Args,
        ) -> impl PinInit<Self, Error> {
            try_pin_init!(GemOnlyObject {})
        }
    }

    #[vtable]
    impl drm::Driver for GemOnlyDriver {
        type Data = GemOnlyData;
        type File = GemOnlyFile;
        type Object = gem::Object<GemOnlyObject>;

        const INFO: drm::DriverInfo = INFO;
        const IOCTLS: &'static [drm::ioctl::DrmIoctlDescriptor] = &[];
    }

    fn create_gem_only_drm_dev() -> Result<(faux::Registration, ARef<drm::Device<GemOnlyDriver>>)> {
        let dev = faux::Registration::new(c"GemOnlyKunit", None)?;
        let drm = drm::Device::<GemOnlyDriver>::new(dev.as_ref(), try_pin_init!(GemOnlyData {}))?;

        Ok((dev, drm))
    }

    #[test]
    #[cfg(CONFIG_KASAN)]
    fn gem_object_mints_device_ref_after_device_free() -> Result {
        use gem::BaseObject;

        let (_dev, drm) = create_gem_only_drm_dev()?;
        let obj = gem::Object::<GemOnlyObject>::new(&drm, crate::page::PAGE_SIZE, ())?;

        assert_eq!(obj.size(), crate::page::PAGE_SIZE);

        drop(drm);

        assert_eq!(obj.size(), crate::page::PAGE_SIZE);
        let _dev_ref = ARef::from(obj.dev());

        Ok(())
    }

    #[pin_data(PinnedDrop)]
    struct DrmPinInitFailData {
        field0: i32,
        field1: i32,
        field2: i32,
    }

    impl DrmPinInitFailData {
        const NOT_INITIALIZED: i32 = 0;
        const INITIALIZED: i32 = 1;
    }

    #[pinned_drop]
    impl PinnedDrop for DrmPinInitFailData {
        fn drop(self: Pin<&mut Self>) {
            pr_err!("We should not get here if pin init fails\n");
            assert_eq!(self.field0, DrmPinInitFailData::INITIALIZED);
            assert_eq!(self.field1, DrmPinInitFailData::INITIALIZED);
            assert_eq!(self.field2, DrmPinInitFailData::INITIALIZED);
        }
    }

    struct DrmPinInitFailDriver;

    struct DrmPinInitFailFile;

    impl drm::file::DriverFile for DrmPinInitFailFile {
        type Driver = DrmPinInitFailDriver;

        fn open(_dev: &drm::Device<DrmPinInitFailDriver>) -> Result<Pin<KBox<Self>>> {
            Ok(KBox::new(Self, GFP_KERNEL)?.into())
        }
    }

    #[pin_data]
    struct DrmPinInitFailObject {}

    impl gem::DriverObject for DrmPinInitFailObject {
        type Driver = DrmPinInitFailDriver;
        type Args = ();

        fn new(
            _dev: &drm::Device<DrmPinInitFailDriver>,
            _size: usize,
            _args: Self::Args,
        ) -> impl PinInit<Self, Error> {
            try_pin_init!(DrmPinInitFailObject {})
        }
    }

    #[vtable]
    impl drm::Driver for DrmPinInitFailDriver {
        type Data = DrmPinInitFailData;
        type File = DrmPinInitFailFile;
        type Object = gem::Object<DrmPinInitFailObject>;

        const INFO: drm::DriverInfo = INFO;
        const IOCTLS: &'static [drm::ioctl::DrmIoctlDescriptor] = &[];
    }

    fn drm_pin_init_fail_data_init() -> impl PinInit<DrmPinInitFailData, Error> {
        try_pin_init!(DrmPinInitFailData {
            field0: DrmPinInitFailData::INITIALIZED,
            field1: Err::<i32, Error>(EINVAL)?,
            field2: DrmPinInitFailData::INITIALIZED,
        }? Error)
    }

    #[test]
    fn device_new_drops_data_after_drm_pin_init_fail() -> Result {
        let dev = faux::Registration::new(c"DrmPinInitFailKunit", None)?;

        let ret =
            drm::Device::<DrmPinInitFailDriver>::new(dev.as_ref(), drm_pin_init_fail_data_init());

        assert!(ret.is_err());

        Ok(())
    }
}
