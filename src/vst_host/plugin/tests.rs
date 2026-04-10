use std::ffi::{CStr, c_char, c_void};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::AtomicUsize;

use vst3::Steinberg::Vst::{IAttributeListTrait, IMessage, IMessageTrait};
use vst3::Steinberg::{IBStream, IBStreamTrait, TUID, kResultOk};
use vst3::{ComPtr, ComWrapper, Interface};

use super::attributes::MemoryStream;
use super::host_impl::*;
use super::LoadedPlugin;
use crate::vst_host::scanner::PluginInfo;

fn guid_to_tuid(guid: [u8; 16]) -> TUID {
    unsafe { std::mem::transmute::<[u8; 16], TUID>(guid) }
}

#[test]
fn host_application_creates_message_and_attribute_objects() {
    let host_application = Box::into_raw(Box::new(HostApplication {
        vtbl: &HOST_APPLICATION_VTBL,
        ref_count: AtomicUsize::new(1),
    })) as *mut c_void;

    let mut message_ptr = ptr::null_mut();
    let message_cid = guid_to_tuid(<IMessage as Interface>::IID);
    let message_iid = guid_to_tuid(<IMessage as Interface>::IID);
    let result = unsafe {
        host_app_create_instance(
            host_application,
            &message_cid,
            &message_iid,
            &mut message_ptr,
        )
    };
    assert_eq!(result, kResultOk);

    let message = unsafe { ComPtr::from_raw(message_ptr as *mut IMessage).unwrap() };
    unsafe {
        message.setMessageID(b"unit-test\0".as_ptr() as *const c_char);
    }
    let message_id = unsafe { CStr::from_ptr(message.getMessageID()) };
    assert_eq!(message_id.to_str().unwrap(), "unit-test");

    let attrs = unsafe { ComPtr::from_raw(message.getAttributes()).unwrap() };
    let key = b"answer\0".as_ptr() as *const c_char;
    assert_eq!(unsafe { attrs.setInt(key, 42) }, kResultOk);

    let mut value = 0i64;
    assert_eq!(unsafe { attrs.getInt(key, &mut value) }, kResultOk);
    assert_eq!(value, 42);

    unsafe {
        host_app_release(host_application);
    }
}

#[test]
fn memory_stream_round_trip() {
    let stream = ComWrapper::new(MemoryStream::default());
    let stream_ptr = stream.to_com_ptr::<IBStream>().unwrap();

    let mut written = 0;
    let mut payload = [1u8, 2, 3, 4];
    assert_eq!(
        unsafe {
            stream_ptr.write(
                payload.as_mut_ptr() as *mut c_void,
                payload.len() as i32,
                &mut written,
            )
        },
        kResultOk
    );
    assert_eq!(written, payload.len() as i32);

    stream.rewind();

    let mut read = 0;
    let mut out = [0u8; 4];
    assert_eq!(
        unsafe {
            stream_ptr.read(out.as_mut_ptr() as *mut c_void, out.len() as i32, &mut read)
        },
        kResultOk
    );
    assert_eq!(read, out.len() as i32);
    assert_eq!(out, payload);
}

#[test]
fn test_load_and_process_vst() {
    let path = std::env::var("TEST_VST_PATH").ok().map(PathBuf::from);
    if let Some(p) = path {
        let mut plugin = LoadedPlugin::load(&PluginInfo {
            path: p,
            name: "Test".into(),
            vendor: "".into(),
            category: "".into(),
        })
        .unwrap();
        plugin.setup_processing(48000.0, 256).unwrap();
        let mut l = vec![0.0f32; 256];
        let mut r = vec![0.0f32; 256];
        let mut buffer = vec![l.as_mut_slice(), r.as_mut_slice()];
        for _ in 0..10 {
            plugin.process_in_place(&mut buffer, 256);
        }
    }
}
