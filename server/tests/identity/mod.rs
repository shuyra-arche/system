#![allow(dead_code)]
extern crate alloc;
pub mod book {
    include!("../../src/system/identity/book/mod.rs");

    mod tests {
        include!("book.rs");
    }
}
mod revision_owner {
    include!("../../src/system/identity/revision.rs");

    mod tests {
        include!("revision.rs");
    }
}

#[path = "../../src/system/identity/service/answer.rs"]
pub mod answer;
pub mod system {
    pub mod identity {
        pub use crate::book;
    }
}
mod dispatch;
