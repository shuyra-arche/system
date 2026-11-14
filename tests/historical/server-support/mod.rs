//! Feature-gated runtime fixtures for provider and Source harness scenarios.
//!
//! The facade owns the runtime registry, plans and cursors; callers get typed
//! operations and views, never the private System modules or raw resources.

pub mod account;
mod action;
pub mod bench;
pub mod copy_scenario;
pub mod daemon_scenario;
pub mod device_confirmation_config;
pub mod device_confirmation_scenario;
pub mod device_guard_scenario;
pub mod device_guard_settings;
pub mod device_ruin;
mod error;
mod fault;
mod fixture;
pub mod hierarchy_scenario;
mod identity_hooks;
pub mod identity_scenario;
mod image;
mod instance_view;
pub mod lifecycle;
mod loaded_unit;
mod loader_fixture;
pub mod loader_scenario;
mod state;
mod supply_hook;
mod supply_resource;
pub mod system_fault;

pub use crate::system::control::unit::start::Error as SceneError;
pub use action::FixtureAction;
pub use device_confirmation_config::ConfirmationConfig;
pub use device_guard_settings::GuardSettings;
pub use error::FixtureError;
pub use fixture::Fixture;
pub use identity_hooks::IdentityHooks;
pub use image::Image;
pub use instance_view::InstanceView;
pub use loaded_unit::LoadedUnit;
pub use loader_fixture::LoaderFixture;
pub use state::FixtureState;
pub use supply_hook::SupplyHook;
