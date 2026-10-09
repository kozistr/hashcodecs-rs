use super::*;
use std::ptr;

#[test]
fn snapshot_before_writes() {
    Python::initialize();
    Python::attach(|py| {
        let owner = PyByteArray::new(py, b"mutable");
        let input = BytesLike::ByteArray(&owner);
        assert_eq!(input.bytearray_identity(), Some(owner.as_ptr()));
        assert!(!input.detach_safe());
        assert_eq!(
            input.snapshot_for_output(&owner).unwrap().unwrap(),
            b"mutable"
        );

        let buffer = acquire_buffer(owner.as_any(), ptr::null_mut()).unwrap();
        let guarded = BytesLike::GuardedBytes {
            bytes: b"mutable".to_vec(),
            buffer,
        };
        assert!(!guarded.buffer_release_may_reenter());
        assert!(!guarded.overlaps(&owner));
        assert_eq!(
            guarded.snapshot_before_output_write(true).unwrap().unwrap(),
            b"mutable"
        );
        drop(guarded);
        owner.resize(1).unwrap();

        let bytes = PyBytes::new(py, b"exported");
        let buffer = acquire_buffer(bytes.as_any(), ptr::null_mut()).unwrap();
        let guarded = BytesLike::GuardedBytes {
            bytes: bytes.as_bytes().to_vec(),
            buffer,
        };
        assert!(guarded.buffer_release_may_reenter());
        let stable = guarded.into_stable_before_callbacks(true).unwrap();
        assert!(matches!(stable, BytesLike::OwnedVec(_)));
        assert_eq!(stable.stable_bytes(), b"exported");
        assert_eq!(BytesLike::Text("ASCII").stable_bytes(), b"ASCII");
    });
}

#[test]
fn reject_noncontiguous_exports() {
    Python::initialize();
    Python::attach(|py| {
        let view = py.eval(c"memoryview(b'abcdef')[::2]", None, None).unwrap();
        let error = buffer_bytes_like(&view, "s", true).err().unwrap();
        assert!(error.is_instance_of::<PyBufferError>(py));

        #[cfg(Py_3_12)]
        {
            let exporter = py.eval(
                c"type('Strided', (), {'__buffer__': lambda self, flags: memoryview(b'abcdef')[::2]})()",
                None,
                None,
            ).unwrap();
            let error = buffer_bytes_like(&exporter, "s", true).err().unwrap();
            assert!(error.is_instance_of::<PyBufferError>(py));
            assert_eq!(
                buffer_bytes_like(&exporter, "s", false)
                    .unwrap()
                    .stable_bytes(),
                b"ace"
            );
        }

        let buffer = acquire_buffer(&view, ptr::null_mut()).unwrap();
        assert_eq!(copy_buffer(py, &buffer).unwrap().as_bytes(), b"ace");
        drop(buffer);
        let error = binascii_ascii_or_bytes_exported(&py.None().into_bound(py))
            .err()
            .unwrap();
        assert!(error.is_instance_of::<PyTypeError>(py));
        assert_eq!(
            error.to_string(),
            "TypeError: argument should be bytes, buffer or ASCII string, not 'NoneType'"
        );

        view.call_method0("release").unwrap();
        let error = binascii_ascii_or_bytes_exported(&view).err().unwrap();
        assert!(error.is_instance_of::<PyValueError>(py));
    });
}

#[test]
fn retain_mutable_slices() {
    Python::initialize();
    Python::attach(|py| {
        let view = py
            .eval(
                c"memoryview(bytearray(b'x' + b'a' * 65536 + b'y'))[1:-1]",
                None,
                None,
            )
            .unwrap();
        let view = view.cast::<PyMemoryView>().unwrap();
        let input = exact_memoryview_bytes_like(view, true).unwrap();
        assert_eq!(input.stable_bytes(), &[b'a'; 65536]);
        assert!(!matches!(input, BytesLike::OwnedByteArray(_)));
        drop(input);
        let owner = view.getattr("obj").unwrap();
        view.call_method0("release").unwrap();
        owner.cast::<PyByteArray>().unwrap().resize(0).unwrap();
    });
}

#[test]
fn encode_string_subclasses() {
    Python::initialize();
    Python::attach(|py| {
        let value = py
            .eval(c"type('Text', (str,), {} )('YWJj')", None, None)
            .unwrap();
        let input = ascii_or_bytes(py, &value, "s").unwrap();
        assert_eq!(input.stable_bytes(), b"YWJj");
    });
}

#[test]
fn read_memoryview_metadata() {
    Python::initialize();
    Python::attach(|py| {
        let owner_data = vec![b'a'; 64 * 1024];
        let owner = PyBytes::new(py, &owner_data);
        let memoryview = PyMemoryView::from(owner.as_any()).unwrap();
        let info = memoryview_info(&memoryview).unwrap();
        assert_eq!(info.nbytes, owner_data.len());
        assert!(info.c_contiguous);
        assert!(info.owner.as_ref().unwrap().is(&owner));
        drop(info);

        let noncontiguous = py
            .eval(c"memoryview(b'abcdef')[::2]", None, None)
            .unwrap()
            .cast_into::<PyMemoryView>()
            .unwrap();
        let info = memoryview_info(&noncontiguous).unwrap();
        assert_eq!(info.nbytes, 3);
        assert!(!info.c_contiguous);
        assert!(info.owner.is_none());
        drop(info);

        memoryview.call_method0(intern!(py, "release")).unwrap();
        let error = memoryview_info(&memoryview).err().unwrap();
        assert!(error.is_instance_of::<PyValueError>(py));
        assert_eq!(
            error.to_string(),
            "ValueError: operation forbidden on released memoryview object"
        );
    });
}

#[test]
fn retain_memoryview_slice() {
    Python::initialize();
    Python::attach(|py| {
        let memoryview = py
            .eval(c"memoryview(b'x' + b'a' * 65536 + b'y')[1:-1]", None, None)
            .unwrap()
            .cast_into::<PyMemoryView>()
            .unwrap();
        let input = exact_memoryview_bytes_like(&memoryview, true).unwrap();
        assert!(matches!(
            input,
            BytesLike::OwnedBytesSlice {
                offset: 1,
                len: 65536,
                ..
            }
        ));
        assert_eq!(input.stable_bytes(), &[b'a'; 65536]);
        let output = PyByteArray::new(py, b"output");
        assert!(!input.overlaps(&output));
        assert!(!BytesLike::Text("text").overlaps(&output));
    });
}
