//! Pure business logic and ports. No UI framework and no Git implementation.

mod graph_layout;
mod model;
mod ports;
mod state;

pub use graph_layout::build_dag;
pub use model::*;
pub use ports::*;
pub use state::*;
