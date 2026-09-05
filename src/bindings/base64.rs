//! Base64 binding owners: API, codecs, policy, storage, batches, and byte scanning.

pub(in crate::bindings) mod api;
mod batch;
mod configured;
mod decode;
mod encode;
mod lenient;
mod policy;
mod scan;
mod staging;
mod strict;

pub(super) use api::add_to_module;
