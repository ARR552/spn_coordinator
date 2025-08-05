// Include the generated protobuf code from src/generated directory
pub mod generated {

    // Only include the files that were actually generated
    include!("generated/types.rs");
    include!("generated/verifier.rs");
    include!("generated/artifact.rs");
    include!("generated/network.rs");}

pub use generated::*;

pub mod types {
    pub use super::generated::*;
}