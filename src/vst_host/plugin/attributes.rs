use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr, CString};
use std::ptr;
use std::slice;
use std::sync::Mutex;

use vst3::ComWrapper;
use vst3::Steinberg::Vst::{IAttributeList, IAttributeListTrait, IMessage, IMessageTrait};
use vst3::Steinberg::{
    int32, int64, kInvalidArgument, kResultFalse, kResultOk, tresult, uint32, IBStream,
    IBStreamTrait,
};

use super::seh_ffi::lock_recover;

#[derive(Clone)]
pub(super) enum AttributeValue {
    Int(int64),
    Float(f64),
    String(Box<[u16]>),
    Binary(Box<[u8]>),
}

#[derive(Default)]
pub(super) struct HostAttributeList {
    values: Mutex<HashMap<Vec<u8>, AttributeValue>>,
}

impl vst3::Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

unsafe fn attr_key(id: *const c_char) -> Option<Vec<u8>> {
    if id.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(id) }.to_bytes().to_vec())
}

unsafe fn read_wide_string(string: *const u16) -> Vec<u16> {
    if string.is_null() {
        return Vec::new();
    }
    let mut len = 0usize;
    while unsafe { *string.add(len) } != 0 {
        len += 1;
    }
    unsafe { slice::from_raw_parts(string, len) }.to_vec()
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: int64,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        lock_recover(&self.values).insert(key, AttributeValue::Int(value));
        kResultOk
    }

    unsafe fn getInt(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: *mut int64,
    ) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        match values.get(&key) {
            Some(AttributeValue::Int(v)) => {
                unsafe {
                    value.write(*v);
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: f64,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        lock_recover(&self.values).insert(key, AttributeValue::Float(value));
        kResultOk
    }

    unsafe fn getFloat(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        value: *mut f64,
    ) -> tresult {
        if value.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        match values.get(&key) {
            Some(AttributeValue::Float(v)) => {
                unsafe {
                    value.write(*v);
                }
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        string: *const vst3::Steinberg::Vst::TChar,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let value = unsafe { read_wide_string(string) };
        lock_recover(&self.values).insert(key, AttributeValue::String(value.into_boxed_slice()));
        kResultOk
    }

    unsafe fn getString(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        string: *mut vst3::Steinberg::Vst::TChar,
        size_in_bytes: uint32,
    ) -> tresult {
        if string.is_null() || size_in_bytes < std::mem::size_of::<u16>() as u32 {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        let Some(AttributeValue::String(value)) = values.get(&key) else {
            return kResultFalse;
        };

        let capacity = (size_in_bytes as usize / std::mem::size_of::<u16>()).max(1);
        let dst = unsafe { slice::from_raw_parts_mut(string, capacity) };
        let copy_len = value.len().min(capacity.saturating_sub(1));
        dst[..copy_len].copy_from_slice(&value[..copy_len]);
        dst[copy_len] = 0;
        kResultOk
    }

    unsafe fn setBinary(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        data: *const c_void,
        size_in_bytes: uint32,
    ) -> tresult {
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        if data.is_null() && size_in_bytes != 0 {
            return kInvalidArgument;
        }
        let bytes = if size_in_bytes == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(data as *const u8, size_in_bytes as usize) }.to_vec()
        };
        lock_recover(&self.values).insert(key, AttributeValue::Binary(bytes.into_boxed_slice()));
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: vst3::Steinberg::Vst::IAttributeList_::AttrID,
        data: *mut *const c_void,
        size_in_bytes: *mut uint32,
    ) -> tresult {
        if data.is_null() || size_in_bytes.is_null() {
            return kInvalidArgument;
        }
        let Some(key) = (unsafe { attr_key(id) }) else {
            return kInvalidArgument;
        };
        let values = lock_recover(&self.values);
        let Some(AttributeValue::Binary(value)) = values.get(&key) else {
            unsafe {
                data.write(ptr::null());
                size_in_bytes.write(0);
            }
            return kResultFalse;
        };
        unsafe {
            data.write(value.as_ptr() as *const c_void);
            size_in_bytes.write(value.len() as uint32);
        }
        kResultOk
    }
}

pub(super) struct HostMessage {
    message_id: Mutex<CString>,
    attributes: ComWrapper<HostAttributeList>,
}

impl Default for HostMessage {
    fn default() -> Self {
        Self {
            message_id: Mutex::new(CString::new("").unwrap()),
            attributes: ComWrapper::new(HostAttributeList::default()),
        }
    }
}

impl vst3::Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> vst3::Steinberg::FIDString {
        lock_recover(&self.message_id).as_ptr()
    }

    unsafe fn setMessageID(&self, id: vst3::Steinberg::FIDString) {
        let value = if id.is_null() {
            CString::new("").unwrap()
        } else {
            CString::new(unsafe { CStr::from_ptr(id).to_bytes() })
                .unwrap_or_else(|_| CString::new("").unwrap())
        };
        *lock_recover(&self.message_id) = value;
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes
            .to_com_ptr::<IAttributeList>()
            .unwrap()
            .into_raw()
    }
}

#[derive(Default)]
pub(super) struct MemoryStream {
    state: Mutex<MemoryStreamState>,
}

#[derive(Default)]
struct MemoryStreamState {
    data: Vec<u8>,
    position: usize,
}

impl MemoryStream {
    pub(super) fn from_bytes(data: Vec<u8>) -> Self {
        Self {
            state: Mutex::new(MemoryStreamState { data, position: 0 }),
        }
    }

    pub(super) fn rewind(&self) {
        lock_recover(&self.state).position = 0;
    }

    pub(super) fn to_vec(&self) -> Vec<u8> {
        lock_recover(&self.state).data.clone()
    }
}

impl vst3::Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_read: *mut int32,
    ) -> tresult {
        if buffer.is_null() || num_bytes < 0 {
            return kInvalidArgument;
        }
        let mut state = lock_recover(&self.state);
        let available = state.data.len().saturating_sub(state.position);
        let requested = num_bytes as usize;
        let count = available.min(requested);

        unsafe {
            ptr::copy_nonoverlapping(
                state.data.as_ptr().add(state.position),
                buffer as *mut u8,
                count,
            );
        }
        state.position += count;

        if !num_bytes_read.is_null() {
            unsafe {
                num_bytes_read.write(count as int32);
            }
        }
        if count == requested {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: int32,
        num_bytes_written: *mut int32,
    ) -> tresult {
        if num_bytes < 0 || (buffer.is_null() && num_bytes != 0) {
            return kInvalidArgument;
        }
        let mut state = lock_recover(&self.state);
        let requested = num_bytes as usize;
        let end = state.position.saturating_add(requested);
        if end > state.data.len() {
            state.data.resize(end, 0);
        }
        if requested != 0 {
            unsafe {
                ptr::copy_nonoverlapping(
                    buffer as *const u8,
                    state.data.as_mut_ptr().add(state.position),
                    requested,
                );
            }
        }
        state.position = end;

        if !num_bytes_written.is_null() {
            unsafe {
                num_bytes_written.write(num_bytes);
            }
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
        let mut state = lock_recover(&self.state);
        let base = match mode {
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekSet => 0i64,
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekCur => {
                state.position as i64
            }
            x if x == vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekEnd => {
                state.data.len() as i64
            }
            _ => return kInvalidArgument,
        };

        let Some(new_pos) = base.checked_add(pos) else {
            return kInvalidArgument;
        };
        if new_pos < 0 {
            return kInvalidArgument;
        }

        state.position = new_pos as usize;
        if !result.is_null() {
            unsafe {
                result.write(new_pos);
            }
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut int64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        unsafe {
            pos.write(lock_recover(&self.state).position as int64);
        }
        kResultOk
    }
}
