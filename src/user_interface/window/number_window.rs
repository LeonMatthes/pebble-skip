use crate::{
	standard_c::{CStr, NotStack},
	Box, Handle,
};
use core::{marker::PhantomData, mem::ManuallyDrop};
#[allow(clippy::wildcard_imports)]
use pebble_sys::{
	standard_c::memory::void,
	user_interface::window::number_window::{NumberWindow as sysNumberWindow, *},
};

use super::{WindowRef, WindowRefMut};

#[repr(C)]
struct NumberWindowDataHeader {
	drop_data: unsafe fn(*mut void),
}

#[repr(C)]
struct NumberWindowDataWrapper<
	I: FnMut(&NumberWindow<void>, &mut T),
	D: FnMut(&NumberWindow<void>, &mut T),
	S: FnMut(&NumberWindow<void>, &mut T),
	T,
> {
	header: NumberWindowDataHeader,
	data: NumberWindowData<I, D, S, T>,
}

unsafe fn drop_number_window_data<
	I: FnMut(&NumberWindow<void>, &mut T),
	D: FnMut(&NumberWindow<void>, &mut T),
	S: FnMut(&NumberWindow<void>, &mut T),
	T,
>(
	data: *mut void,
) {
	let _ =
		Box::<NumberWindowDataWrapper<I, D, S, T>>::from_raw(&mut *(data as *mut NumberWindowDataWrapper<I, D, S, T>));
}

pub struct NumberWindow<'a, T>(
	pub(crate) Handle<'a, sysNumberWindow<'a>>,
	PhantomData<T>,
	*mut void,
	bool,
);

pub struct NumberWindowData<
	I: FnMut(&NumberWindow<void>, &mut T),
	D: FnMut(&NumberWindow<void>, &mut T),
	S: FnMut(&NumberWindow<void>, &mut T),
	T,
> {
	pub incremented: I,
	pub decremented: D,
	pub selected: S,
	pub context: T,
}

impl<'a, T> NumberWindow<'a, T> {
	// TODO: This probably should take and set a set of window handlers, which can then also act as lifecycle hooks for the context.
	/// # Errors
	///
	/// TODO
	///
	pub fn new<
		I: 'a + FnMut(&NumberWindow<void>, &mut T),
		D: 'a + FnMut(&NumberWindow<void>, &mut T),
		S: 'a + FnMut(&NumberWindow<void>, &mut T),
	>(
		label: &'a CStr<impl NotStack>,
		number_window_data: NumberWindowData<I, D, S, T>,
	) -> Result<Self, NumberWindowData<I, D, S, T>>
	where
		T: 'a,
	{
		#![allow(clippy::items_after_statements)]

		let window_data_wrapper = Box::new(NumberWindowDataWrapper {
			header: NumberWindowDataHeader {
				drop_data: drop_number_window_data::<I, D, S, T>,
			},
			data: number_window_data,
		})
		.map_err(|wrapper| wrapper.data)?;
		let window_data_wrapper = Box::leak(window_data_wrapper) as *mut _ as *mut void;

		extern "C" fn raw_incremented<
			'a,
			I: FnMut(&NumberWindow<void>, &mut T),
			D: FnMut(&NumberWindow<void>, &mut T),
			S: FnMut(&NumberWindow<void>, &mut T),
			T,
		>(
			raw_window: &'a mut sysNumberWindow<'a>,
			context: &mut void,
		) {
			let context = context as *mut void; // This will be aliased.
			let fake_window = unsafe {
				//SAFETY: It's actually *kind of* safe to alias NumberWindow instances... But only because they store a Handle internally, which stores a pointer.
				// Actually accessing associated data would NOT be safe, so the user-provided handlers only see a NumberWindow<void> where such access is impossible.
				#[allow(clippy::cast_ptr_alignment)]
				NumberWindow::<void>::from_raw_alias(raw_window, context)
			};
			unsafe {
				let wrapper = &mut *(context as *mut NumberWindowDataWrapper<I, D, S, T>);
				(wrapper.data.incremented)(&fake_window, &mut wrapper.data.context);
			}
			NumberWindow::abandon(fake_window);
		}
		extern "C" fn raw_decremented<
			'a,
			I: FnMut(&NumberWindow<void>, &mut T),
			D: FnMut(&NumberWindow<void>, &mut T),
			S: FnMut(&NumberWindow<void>, &mut T),
			T,
		>(
			raw_window: &'a mut sysNumberWindow<'a>,
			context: &mut void,
		) {
			let context = context as *mut void; // This will be aliased.
			let fake_window = unsafe {
				//SAFETY: It's actually *kind of* safe to alias NumberWindow instances... But only because they store a Handle internally, which stores a pointer.
				// Actually accessing associated data would NOT be safe, so the user-provided handlers only see a NumberWindow<void> where such access is impossible.
				#[allow(clippy::cast_ptr_alignment)]
				NumberWindow::<void>::from_raw_alias(raw_window, context)
			};
			unsafe {
				let wrapper = &mut *(context as *mut NumberWindowDataWrapper<I, D, S, T>);
				(wrapper.data.decremented)(&fake_window, &mut wrapper.data.context);
			}
			NumberWindow::abandon(fake_window);
		}
		extern "C" fn raw_selected<
			'a,
			I: FnMut(&NumberWindow<void>, &mut T),
			D: FnMut(&NumberWindow<void>, &mut T),
			S: FnMut(&NumberWindow<void>, &mut T),
			T,
		>(
			raw_window: &'a mut sysNumberWindow<'a>,
			context: &mut void,
		) {
			let context = context as *mut void; // This will be aliased.
			let fake_window = unsafe {
				//SAFETY: It's actually *kind of* safe to alias NumberWindow instances... But only because they store a Handle internally, which stores a pointer.
				// Actually accessing associated data would NOT be safe, so the user-provided handlers only see a NumberWindow<void> where such access is impossible.
				#[allow(clippy::cast_ptr_alignment)]
				NumberWindow::<void>::from_raw_alias(raw_window, context)
			};
			unsafe {
				let wrapper = &mut *(context as *mut NumberWindowDataWrapper<I, D, S, T>);
				(wrapper.data.selected)(&fake_window, &mut wrapper.data.context);
			}
			NumberWindow::abandon(fake_window);
		}

		match unsafe {
			number_window_create(
				label.as_c_str(),
				NumberWindowCallbacks {
					incremented: Some(raw_incremented::<I, D, S, T>),
					decremented: Some(raw_decremented::<I, D, S, T>),
					selected: Some(raw_selected::<I, D, S, T>),
				},
				&mut *window_data_wrapper,
			)
		} {
			Some(raw_window) => Ok(Self(
				Handle::new(raw_window),
				PhantomData,
				window_data_wrapper,
				true,
			)),
			None => Err(Box::into_inner(unsafe {
				Box::<NumberWindowDataWrapper<I, D, S, T>>::from_raw(
					&mut *(window_data_wrapper as *mut NumberWindowDataWrapper<I, D, S, T>),
				)
			})
			.data),
		}
	}

	/// Assembles a new instance of [`NumberWindow`] from the given raw window handle.
	///
	/// # Safety
	///
	/// This function is only safe if `raw_window` is a raw window handle that was previously [`.leak()`]ed from the same [`NumberWindow<T>`] variant and no other [`NumberWindow<T>`] instance has been created from it since.
	///
	/// [`.leak()`]: #method.leak
	pub unsafe fn from_raw(raw_window: &'a mut sysNumberWindow<'a>, number_window_data_wrapper: *mut void) -> Self {
		Self(
			Handle::new(raw_window),
			PhantomData,
			number_window_data_wrapper,
			true,
		)
	}

	/// Leaks the current [`NumberWindow`] instance into a raw Pebble number window handle.
	///
	/// Note that [`NumberWindow`] has associated heap instances beyond the raw window, so only destroying that would still leak memory.
	#[must_use = "Not reassembling the `NumberWindow` later causes a memory leak."]
	pub fn leak(self) -> (&'a mut sysNumberWindow<'a>, *mut void)
	where
		T: 'a,
	{
		let undropped = ManuallyDrop::new(self);
		unsafe { (undropped.0.duplicate().unwrap(), undropped.2) }
	}
}

impl<'a, T> NumberWindow<'a, T> {
	#[must_use]
	pub fn window(&self) -> WindowRef<'_> {
		WindowRef(Handle::new(unsafe {
			number_window_get_window_mut(&mut *(self.0.as_mut_unchecked() as *mut _))
		}))
	}

	#[must_use]
	pub fn window_mut<'b: 'a>(&'b mut self) -> WindowRefMut<'b> {
		WindowRefMut(Handle::new(unsafe {
			number_window_get_window_mut(self.0.as_mut_unchecked())
		}))
	}

	pub fn set_label(&self, label: &'a CStr<impl NotStack>) {
		unsafe { number_window_set_label(self.0.as_mut_unchecked(), label.as_c_str()) }
	}

	pub fn set_max(&self, max: i32) {
		unsafe { number_window_set_max(self.0.as_mut_unchecked(), max) }
	}

	pub fn set_min(&self, min: i32) {
		unsafe { number_window_set_min(self.0.as_mut_unchecked(), min) }
	}

	pub fn set_value(&self, value: i32) {
		unsafe { number_window_set_value(self.0.as_mut_unchecked(), value) }
	}

	pub fn set_step_size(&self, step_size: i32) {
		unsafe { number_window_set_step_size(self.0.as_mut_unchecked(), step_size) }
	}

	#[must_use]
	pub fn get_value(&self) -> i32 {
		unsafe { number_window_get_value(&*self.0) }
	}

	unsafe fn from_raw_alias(raw_window: &'a mut sysNumberWindow<'a>, number_window_data_wrapper: *mut void) -> Self {
		Self(
			Handle::new(raw_window),
			PhantomData,
			number_window_data_wrapper,
			false,
		)
	}

	/// Discards this instance while skipping the destructor. Helper for aliased temporaries.
	fn abandon(self) {
		let _ = ManuallyDrop::new(self);
	}
}

impl<'a, T> Drop for NumberWindow<'a, T> {
	fn drop(&mut self) {
		if !self.3 {
			return;
		}

		unsafe {
			//SAFETY: window_data is created and leaked in the only accessible constructor.
			//SAFETY: self.0 isn't accessed after this.
			let data_wrapper = self.2;
			let drop_data = (*(data_wrapper as *mut NumberWindowDataHeader)).drop_data;
			// Detaching the lifetime here takes a bit of work.
			let sys_number_window = self.0.duplicate().unwrap() as *mut _ as *mut void as *mut _;

			// Destroy the window, THEN drop its data.
			number_window_destroy(&mut *sys_number_window);
			drop_data(data_wrapper);
		}
	}
}
