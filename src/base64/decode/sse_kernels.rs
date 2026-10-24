macro_rules! decode_kernels {
    ($features:literal, $errors:ident => $has_errors:expr) => {
        #[target_feature(enable = $features)]
        pub(crate) unsafe fn decode<A: Decoder, S: Store>(
            input: &[u8],
            output: *mut u8,
        ) -> Result<(usize, usize), Base64Error> {
            let mut source = 0;
            let mut destination = 0;

            while source + 64 <= input.len() {
                let (first, first_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
                let (second, second_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
                let (third, third_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
                let (fourth, fourth_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
                let $errors = _mm_or_si128(
                    _mm_or_si128(first_errors, second_errors),
                    _mm_or_si128(third_errors, fourth_errors),
                );

                if A::rejects_input($has_errors) {
                    return Err(Base64Error::InvalidInput);
                }

                // The whole group is valid. Each following store replaces the previous
                // store's four padding bytes; only the final store needs the boundary policy.
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination), pack_16_indices(first)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination + 12), pack_16_indices(second)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination + 24), pack_16_indices(third)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { S::store_12(output.add(destination + 36), pack_16_indices(fourth)) };
                source += 64;
                destination += 48;
            }

            while source + 16 <= input.len() {
                let (indices, $errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

                if A::rejects_input($has_errors) {
                    return Err(Base64Error::InvalidInput);
                }

                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { S::store_12(output.add(destination), pack_16_indices(indices)) };
                source += 16;
                destination += 12;
            }

            Ok((source, destination))
        }

        #[target_feature(enable = $features)]
        pub(crate) fn validate<A: Decoder>(input: &[u8]) -> Result<usize, Base64Error> {
            let mut source = 0;

            while source + 64 <= input.len() {
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                let (_, first) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                let (_, second) = unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                let (_, third) = unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                let (_, fourth) = unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
                let $errors =
                    _mm_or_si128(_mm_or_si128(first, second), _mm_or_si128(third, fourth));

                if $has_errors {
                    return Err(Base64Error::InvalidInput);
                }

                source += 64;
            }

            while source + 16 <= input.len() {
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                let (_, $errors) = unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

                if $has_errors {
                    return Err(Base64Error::InvalidInput);
                }

                source += 16;
            }

            Ok(source)
        }

        #[target_feature(enable = $features)]
        pub(crate) unsafe fn decode_prefix<A: Decoder>(
            input: &[u8],
            output: *mut u8,
        ) -> (usize, usize) {
            let mut source = 0;
            let mut destination = 0;

            while source + 64 <= input.len() {
                let (first, first_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source)) };
                let (second, second_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 16)) };
                let (third, third_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 32)) };
                let (fourth, fourth_errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source + 48)) };
                let $errors = _mm_or_si128(
                    _mm_or_si128(first_errors, second_errors),
                    _mm_or_si128(third_errors, fourth_errors),
                );

                if $has_errors {
                    break;
                }

                // All four blocks are valid, so overlapping stores stay within this prefix.
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination), pack_16_indices(first)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination + 12), pack_16_indices(second)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_padded(output.add(destination + 24), pack_16_indices(third)) };
                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_exact(output.add(destination + 36), pack_16_indices(fourth)) };
                source += 64;
                destination += 48;
            }

            while source + 16 <= input.len() {
                let (indices, $errors) =
                    // SAFETY: Loop bounds or tail masks keep input loads in range; the caller
                    // reserves the decoded capacity required by the store policy and enables the
                    // kernel features.
                    unsafe { A::decode_indices_16(input.as_ptr().add(source)) };

                if $has_errors {
                    break;
                }

                // SAFETY: Loop bounds or tail masks keep input loads in range; the caller reserves
                // the decoded capacity required by the store policy and enables the kernel
                // features.
                unsafe { store_12_exact(output.add(destination), pack_16_indices(indices)) };
                source += 16;
                destination += 12;
            }

            (source, destination)
        }
    };
}

pub(super) use decode_kernels;
