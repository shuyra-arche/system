use kernel_api::{
    boot::{
        args,
        capsule::Capsule,
        ledger::{Call, Entries, Name, Page, ResourceKind},
    },
    model::{identity::PendId, unit::ControlId, view::ViewKind},
};

/// The two independently authorized halves of a Kernel dispatch resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    pub view: (ViewKind, PendId),
    pub sign: PendId,
}

/// The handle domain is preserved when decoding the startup ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    Control(ControlId),
    Permission(PendId),
    View(ViewKind, PendId),
    Sign(PendId),
    Nest(PendId),
    Dispatch(Dispatch),
}

/// Borrowed immutable Kernel boot facts; this object owns no capabilities.
pub struct Accounts<'a> {
    view: &'a [u8],
    entries: Entries<'a>,
}

impl<'a> Accounts<'a> {
    pub fn parse(view: &'a [u8], ledger: &'a [u8]) -> Option<Self> {
        let archive = unit_contract::bundle::Archive::parse(view).ok()?;
        Capsule::parse(archive.capsule())?;
        Some(Self {
            view,
            entries: Entries::new(ledger)?,
        })
    }

    /// Decode exactly the current Kernel's four-word boot arguments.
    ///
    /// # Safety
    /// The two ranges must be initialized, immutable, readable mappings lasting
    /// for `'a`, from this Task's trusted Kernel startup. Shape checks cannot
    /// establish the validity of arbitrary addresses supplied by another Task.
    pub unsafe fn from_words(words: &[usize]) -> Option<Self> {
        if words.len() != args::LEN {
            return None;
        }
        fn range(base: usize, len: usize) -> Option<()> {
            (base != 0 && len != 0 && len <= isize::MAX as usize && base.checked_add(len).is_some())
                .then_some(())
        }
        range(words[args::VIEW], words[args::VIEW_LEN])?;
        range(words[args::LEDGER], words[args::LEDGER_LEN])?;
        // SAFETY: the trusted caller supplies the mapping/lifetime guarantees.
        let view = unsafe {
            core::slice::from_raw_parts(words[args::VIEW] as *const u8, words[args::VIEW_LEN])
        };
        let ledger = unsafe {
            core::slice::from_raw_parts(words[args::LEDGER] as *const u8, words[args::LEDGER_LEN])
        };
        Self::parse(view, ledger)
    }

    pub fn view(&self) -> &'a [u8] {
        self.view
    }

    pub fn resource(&self, name: Name) -> Option<Resource> {
        let entry = self.entries.find(name)?;
        let pend = |raw: usize| PendId::from_bytes(&(raw as u64).to_le_bytes());
        if let Some(((kind, view), sign)) = entry.dispatch() {
            return Some(Resource::Dispatch(Dispatch {
                view: (kind, pend(view)?),
                sign: pend(sign)?,
            }));
        }
        Some(match entry.kind() {
            ResourceKind::Control => Resource::Control(ControlId::new(entry.handle())),
            ResourceKind::None => Resource::Permission(pend(entry.handle())?),
            ResourceKind::Share => Resource::View(ViewKind::Share, pend(entry.handle())?),
            ResourceKind::Clone => Resource::View(ViewKind::Clone, pend(entry.handle())?),
            ResourceKind::Sign => Resource::Sign(pend(entry.handle())?),
            ResourceKind::Nest => Resource::Nest(pend(entry.handle())?),
        })
    }

    pub fn control(&self, call: Call) -> Option<ControlId> {
        match self.resource(Name::Call(call))? {
            Resource::Control(id) => Some(id),
            _ => None,
        }
    }

    pub fn permission(&self, call: Call) -> Option<PendId> {
        match self.resource(Name::Call(call))? {
            Resource::Permission(id) => Some(id),
            _ => None,
        }
    }

    pub fn page(&self, page: Page) -> Option<PendId> {
        match self.resource(Name::Page(page))? {
            Resource::View(ViewKind::Share, id) => Some(id),
            _ => None,
        }
    }

    pub fn dispatch(&self, name: Name) -> Option<Dispatch> {
        match self.resource(name)? {
            Resource::Dispatch(pair) => Some(pair),
            _ => None,
        }
    }

    pub fn entries(&self) -> impl Iterator<Item = kernel_api::boot::ledger::Entry> + '_ {
        self.entries.iter()
    }
}
