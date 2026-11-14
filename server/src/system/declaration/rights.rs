//! Declaration intent is translated by operation name, never by old mask bits.
use kernel_api::model::{
    modifiable::Modifiable,
    pend::{
        CloneRights, CreationRights, Distributable, FlatRights, Limits, NestRights, NestScope,
        Operations, PendRange, PendScope, ShareRights,
    },
    sign::SignRights,
};
use unit_contract::document::rights as d;

pub(super) fn compile(value: &d::Rights) -> Option<Limits> {
    if !value.valid() {
        return None;
    }
    macro_rules! set {
        ($values:expr, $type:ident, $($variant:path => $right:ident),+ $(,)?) => {{
            let mut result = $type::empty();
            for value in $values { result |= match value { $($variant => $type::$right),+ }; }
            result
        }};
    }
    let operations = match &value.operations {
        d::Operations::None(v) => Operations::None(set!(v, CreationRights,
            d::Creation::Flat => FLAT, d::Creation::Nest => NEST,
            d::Creation::Clone => CLONE, d::Creation::Share => SHARE, d::Creation::Sign => SIGN)),
        d::Operations::Clone(v) => Operations::Clone(set!(v, CloneRights,
            d::Clone::Push => PUSH, d::Clone::Pull => PULL,
            d::Clone::Peek => PEEK, d::Clone::Delete => DELETE)),
        d::Operations::Share(v) => {
            let mut rights = ShareRights::empty();
            for operation in v {
                rights |= match operation {
                    d::Share::Associate => ShareRights::ASSOCIATE,
                    d::Share::Delete => ShareRights::DELETE,
                    d::Share::Open | d::Share::Shut => return None,
                };
            }
            Operations::Share(rights)
        }
        d::Operations::Sign(v) => Operations::Sign(set!(v, SignRights,
            d::Sign::Listen => LISTEN, d::Sign::Trigger => TRIGGER, d::Sign::Route => ROUTE,
            d::Sign::Consume => CONSUME, d::Sign::Delete => DELETE)),
        d::Operations::Flat(v) => Operations::Flat(set!(v, FlatRights,
            d::Flat::Inspect => INSPECT, d::Flat::Transfer => TRANSFER,
            d::Flat::Associate => ASSOCIATE, d::Flat::Share => SHARE,
            d::Flat::Delete => DELETE, d::Flat::Block => BLOCK)),
        d::Operations::Nest(v) => {
            // A declaration cannot name a live ReservationId. Reservation
            // operations require provider-selected, recipient-local authority.
            let mut rights = NestRights::empty();
            for operation in v {
                rights |= match operation {
                    d::Nest::Inspect => NestRights::INSPECT,
                    d::Nest::Reserve => NestRights::RESERVE,
                    d::Nest::Attach => NestRights::ATTACH,
                    d::Nest::Delete => NestRights::DELETE,
                    _ => return None,
                };
            }
            Operations::Nest(rights)
        }
        d::Operations::Memo(_) => return None,
    };
    let range = match value.window {
        d::Window::Inherit => PendRange::Inherit,
        d::Window::Pages { offset, count } => {
            PendRange::pages(usize::try_from(offset).ok()?, usize::try_from(count).ok()?)?
        }
    };
    let scope = match operations {
        Operations::Share(_) | Operations::Flat(_) => PendScope::Pages(range),
        Operations::Nest(_) if range == PendRange::Inherit => PendScope::Nest(NestScope::Whole),
        Operations::None(_) | Operations::Clone(_) | Operations::Sign(_)
            if range == PendRange::Inherit =>
        {
            PendScope::Object
        }
        _ => return None,
    };
    let limits = Limits {
        operations,
        modifiable: if value.modifiable {
            Modifiable::Yes
        } else {
            Modifiable::No
        },
        distributable: if value.distributable {
            Distributable::Yes
        } else {
            Distributable::No
        },
        scope,
    };
    limits.valid().then_some(limits)
}
