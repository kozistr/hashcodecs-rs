use core::fmt;

/// An error returned by a Base64 operation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Base64Error {
    /// The input is not valid padded Base64 for the selected alphabet.
    InvalidInput,
    /// The destination cannot hold the complete result.
    OutputTooSmall {
        /// The minimum destination length.
        required: usize,
        /// The supplied destination length.
        provided: usize,
    },
}

impl fmt::Display for Base64Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => formatter.write_str("invalid Base64 input"),
            Self::OutputTooSmall { required, provided } => write!(
                formatter,
                "Base64 output requires {required} bytes but the destination has {provided}"
            ),
        }
    }
}

impl std::error::Error for Base64Error {}
