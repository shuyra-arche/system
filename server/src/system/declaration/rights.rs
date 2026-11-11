//! Declaration intent is translated by operation name, never by old mask bits.
use kernel_api::model::{
    modifiable::Modifiable,
    pend::{
        CloneRights, CreationRights, Distributable, FlatRights, Limits, MemoRights, NestRights,
        Operations, PendRange, ShareRights,
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
        d::Operations::Share(v) => Operations::Share(set!(v, ShareRights,
            d::Share::Open => OPEN, d::Share::Shut => SHUT,
            d::Share::Associate => ASSOCIATE, d::Share::Delete => DELETE)),
        d::Operations::Sign(v) => Operations::Sign(set!(v, SignRights,
            d::Sign::Listen => LISTEN, d::Sign::Trigger => TRIGGER, d::Sign::Route => ROUTE,
            d::Sign::Consume => CONSUME, d::Sign::Delete => DELETE)),
        d::Operations::Flat(v) => Operations::Flat(set!(v, FlatRights,
            d::Flat::Inspect => INSPECT, d::Flat::Transfer => TRANSFER,
            d::Flat::Associate => ASSOCIATE, d::Flat::Share => SHARE,
            d::Flat::Delete => DELETE, d::Flat::Block => BLOCK)),
        d::Operations::Nest(v) => Operations::Nest(set!(v, NestRights,
            d::Nest::Inspect => INSPECT, d::Nest::Reserve => RESERVE, d::Nest::Release => RELEASE,
            d::Nest::Attach => ATTACH, d::Nest::Transfer => TRANSFER, d::Nest::Publish => PUBLISH,
            d::Nest::Retract => RETRACT, d::Nest::Protect => PROTECT, d::Nest::Delete => DELETE)),
        d::Operations::Memo(v) => Operations::Memo(set!(v, MemoRights,
            d::Memo::Inspect => INSPECT, d::Memo::Insert => INSERT, d::Memo::Remove => REMOVE,
            d::Memo::Publish => PUBLISH, d::Memo::Retract => RETRACT,
            d::Memo::Protect => PROTECT, d::Memo::Delete => DELETE)),
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
        range: match value.window {
            d::Window::Inherit => PendRange::Inherit,
            d::Window::Pages { offset, count } => {
                PendRange::pages(usize::try_from(offset).ok()?, usize::try_from(count).ok()?)?
            }
        },
    };
    limits.valid().then_some(limits)
}
