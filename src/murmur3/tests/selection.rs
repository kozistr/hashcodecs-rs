use crate::backend::Capabilities;
use crate::backend::CpuFeature::{Avx2 as Avx2Feature, Sse41 as Sse41Feature};
use crate::murmur3::dispatch::{
    self,
    Backend::{self, Avx2, Scalar, Sse41},
};

fn assert_selection(select: fn(usize, Capabilities) -> Backend, cases: &[(usize, [Backend; 4])]) {
    let features = [
        &[][..],
        &[Sse41Feature][..],
        &[Avx2Feature][..],
        &[Sse41Feature, Avx2Feature][..],
    ];

    for &(length, expected) in cases {
        for (features, expected) in features.into_iter().zip(expected) {
            assert_eq!(
                select(length, Capabilities::from_features(features)),
                expected,
                "length={length} features={features:?}",
            );
        }
    }
}

#[test]
fn select_x86_32() {
    assert_selection(
        dispatch::select_x86_32_backend,
        &[
            (0, [Scalar; 4]),
            (15, [Scalar; 4]),
            (16, [Scalar, Sse41, Scalar, Sse41]),
            (31, [Scalar, Sse41, Scalar, Sse41]),
            (32, [Scalar, Sse41, Avx2, Avx2]),
            (33, [Scalar, Sse41, Avx2, Avx2]),
            (usize::MAX, [Scalar, Sse41, Avx2, Avx2]),
        ],
    );
}

#[test]
fn select_x86_128() {
    assert_selection(
        dispatch::select_x86_128_backend,
        &[
            (0, [Scalar; 4]),
            (127, [Scalar; 4]),
            (128, [Scalar, Scalar, Avx2, Avx2]),
            (129, [Scalar, Scalar, Avx2, Avx2]),
            (255, [Scalar, Scalar, Avx2, Avx2]),
            (256, [Scalar, Scalar, Avx2, Avx2]),
            (257, [Scalar, Scalar, Avx2, Avx2]),
            (16 * 1024 * 1024 - 1, [Scalar, Scalar, Avx2, Avx2]),
            (16 * 1024 * 1024, [Scalar, Sse41, Avx2, Avx2]),
            (16 * 1024 * 1024 + 1, [Scalar, Sse41, Avx2, Avx2]),
            (usize::MAX, [Scalar, Sse41, Avx2, Avx2]),
        ],
    );
}

#[test]
fn select_x64_128() {
    assert_selection(
        dispatch::select_x64_128_backend,
        &[
            (0, [Scalar; 4]),
            (511, [Scalar; 4]),
            (512, [Scalar, Sse41, Avx2, Avx2]),
            (513, [Scalar, Sse41, Avx2, Avx2]),
            (8 * 1024 * 1024 - 1, [Scalar, Sse41, Avx2, Avx2]),
            (8 * 1024 * 1024, [Scalar, Sse41, Avx2, Avx2]),
            (8 * 1024 * 1024 + 1, [Scalar, Scalar, Avx2, Avx2]),
            (usize::MAX, [Scalar, Scalar, Avx2, Avx2]),
        ],
    );
}
