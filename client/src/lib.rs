//! Task-local System protocol v2 calls over Twine Clone queues.
#![no_std]

extern crate alloc;

mod call;

pub use call::{Cleanup, CleanupError, Error, Failure, Pending, Response};
pub use kernel_api::model::wait::Wait;
pub use system_api as api;

macro_rules! service {
    ($module:ident, $interface:path, $domain:literal $(, { $($item:item)* })?) => {
        pub mod $module {
            use kernel_api::model::identity::{PendId, TaskId};
            use weave::Operation;

            /// Borrows a service endpoint and creation license in the current Task.
            pub struct Client(crate::call::Connection);

            impl Client {
                /// The endpoint must belong to `peer` and permit PUSH and PEEK.
                /// The license must permit creation of Clone queues and Signs.
                pub fn bind(
                    entry: PendId,
                    peer: TaskId,
                    license: PendId,
                ) -> Result<Self, crate::Error> {
                    crate::call::Connection::bind(entry, peer, license).map(Self)
                }

                /// Submit once. The returned call owns the fresh reply route.
                /// Reuse one absolute deadline for sending and receiving.
                #[allow(
                    clippy::result_large_err,
                    reason = "retain cleanup ownership without allocating on an error path"
                )]
                pub fn begin<O: Operation<Interface = $interface>>(
                    &mut self,
                    request: O::Request,
                    wait: crate::Wait,
                ) -> Result<crate::Pending<O>, crate::Failure> {
                    self.0.begin::<O, $domain>(request, wait)
                }
            }
            $($($item)*)?
        }
    };
}

service!(control, system_api::control::operations::Interface, 1);
service!(identity, system_api::identity::operations::Interface, 2, {
    mod query;
    pub use query::{QueryError, TaskQuery};
});
service!(operator, system_api::operator::operations::Interface, 3);
service!(loader, system_api::loader::operations::Interface, 4);
service!(supply, system_api::supply::operations::Interface, 5);
service!(
    authority,
    system_api::operator::authority::operations::Interface,
    6
);
service!(retirement, system_api::retirement::operations::Interface, 7);
service!(setup, system_api::operator::setup::operations::Interface, 8);
service!(
    export,
    system_api::identity::export::operations::Interface,
    9
);
