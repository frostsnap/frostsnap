//! We want to create a FrostRunner which emulates a FrostKey but carries out entire communication rounds
//!
//! The IO occurs outside the FrostRunner, messages are passed in and out of the algorithm:
//! We want something like
//! ```
//! let mut runner = FrostRunner::new(threshold=2, n_parties=2);
//!
//! while !found_enough_parties {
//!     response = runner.find_others(get_new_messages());
//! }
//!
//! // Run these in a response loop also like above
//! let (secret, frost_key) = runner.keygen();
//! let nonce = runner.gen_nonce();
//! let signature = runner.sign(message, secret, secret_nonce, signing_group);
//! ```
//!
//! The runner maintains some certain state
//!
//! struct FrostRunner {
//!     parties: ,
//!     threshold: usize,
//!     n_parties: usize,
//!
//!     // Set after keygen
//!     our_index: Option<u32>,
//!     frost_key: Option<FrostKey>,
//!
//!     // For signing sessions
//!     signing_session,
//! }
//!
