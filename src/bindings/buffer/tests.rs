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

        let buffer = acquire_buffer(owner.as_any(), ptr::null_mut(), ffi::PyBUF_FULL_RO).unwrap();
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
        let buffer = acquire_buffer(bytes.as_any(), ptr::null_mut(), ffi::PyBUF_FULL_RO).unwrap();
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

        let mut buffer = acquire_buffer(&view, ptr::null_mut(), ffi::PyBUF_FULL_RO).unwrap();
        assert_eq!(copy_buffer(py, &mut buffer).unwrap().as_bytes(), b"ace");
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
fn retain_inline_buffer_metadata_after_moves() {
    Python::initialize();
    Python::attach(|py| {
        for bytes in [b"".as_slice(), b"YWJj"] {
            for owner in [
                PyBytes::new(py, bytes).into_any(),
                PyByteArray::new(py, bytes).into_any(),
            ] {
                let buffer = acquire_buffer(&owner, ptr::null_mut(), ffi::PyBUF_FULL_RO).unwrap();
                // Moving through heap storage makes the export's original
                // stack address unavailable, even in optimized builds.
                let mut moved = vec![buffer];
                let inline_shape = moved[0].metadata & INLINE_SHAPE != 0;
                let inline_strides = moved[0].metadata & INLINE_STRIDES != 0;
                moved[0].with_view(|view| {
                    if inline_shape {
                        assert_eq!(view.shape, &raw mut view.len);
                    }

                    if inline_strides {
                        assert_eq!(view.strides, &raw mut view.itemsize);
                    }

                    assert_eq!(unsafe { *view.shape }, bytes.len() as isize);
                    assert_eq!(unsafe { *view.strides }, 1);
                    assert_ne!(unsafe { ffi::PyBuffer_IsContiguous(view, b'C' as _) }, 0);
                });
                let mut buffer = moved.pop().unwrap();
                drop(moved);
                assert_eq!(copy_buffer(py, &mut buffer).unwrap().as_bytes(), bytes);
                drop(buffer);

                if let Ok(owner) = owner.cast::<PyByteArray>() {
                    owner.resize(1).unwrap();
                }
            }
        }
    });
}

#[test]
fn copy_buffer_layouts() {
    Python::initialize();
    Python::attach(|py| {
        let views = py
            .eval(
                c"[
                    memoryview(b''),
                    memoryview(b'abcdef')[::2],
                    memoryview(b'abcdef')[::-1],
                    memoryview(b'ab')[::2],
                    memoryview(b'abcdef').cast('B', shape=[2, 3]),
                    memoryview(b'abcd').cast('I', shape=[]),
                    memoryview(__import__('array').array('H', [1, 2, 3])),
                ]",
                None,
                None,
            )
            .unwrap();

        for view in views.try_iter().unwrap() {
            let view = view.unwrap();
            let c_contiguous = view
                .getattr("c_contiguous")
                .unwrap()
                .extract::<bool>()
                .unwrap();
            let expected = view.call_method0("tobytes").unwrap();
            let expected = expected.cast::<PyBytes>().unwrap();
            let mut buffer = acquire_buffer(&view, ptr::null_mut(), ffi::PyBUF_FULL_RO).unwrap();
            assert_eq!(buffer.metadata & C_CONTIGUOUS != 0, c_contiguous);
            assert_eq!(
                copy_buffer(py, &mut buffer).unwrap().as_bytes(),
                expected.as_bytes()
            );
        }
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
