//! Network discovery tests using Docker-in-Docker
//!
//! These tests verify network topology discovery functionality using
//! a shared Docker-in-Docker container for isolation and consistency.

pub mod shared_dind;
pub mod basic_tests;
pub mod setup;