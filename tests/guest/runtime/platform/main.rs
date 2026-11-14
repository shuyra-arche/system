//! CPU-only QEMU memory qualification for the System runtime component guest.
#![no_std]
#![no_main]
extern crate alloc;
use kernel::entry::boot::{
    self,
    memory::topology::{Fabric, Ram, Topology},
};

#[unsafe(no_mangle)]
extern "C" fn main(hart: usize, dtp: usize) -> ! {
    let result = unsafe { platform_riscv::boot::init(hart, dtp) }.and_then(|_| {
        let tree = unsafe { fdt::Fdt::from_ptr(dtp as *const u8) }.unwrap();
        assert!(
            tree.root()
                .compatible()
                .all()
                .any(|v| matches!(v, "riscv-virtio,qemu" | "riscv-virtio"))
        );
        assert!(matches!(tree.cpus().count(), 1 | 4));
        let regions: alloc::vec::Vec<_> = tree.memory().regions().collect();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].starting_address as usize, 0x80000000);
        assert_eq!(regions[0].size, Some(128 * 1024 * 1024));
        let bytes = unsafe { core::slice::from_raw_parts(dtp as *const u8, tree.total_size()) };
        let admitted = platform_riscv::memory::parse(bytes, dtp, boot::image_region()).unwrap();
        let ram = admitted
            .identity()
            .iter()
            .map(|r| Ram {
                base: r.base as u64,
                bytes: r.size as u64,
            })
            .collect();
        // SAFETY: paired only with run.py's one/four-Hart QEMU virt, normal RAM,
        // no passthrough, hotplug or external requester. All emulated CPUs share
        // coherent normal little-endian RAM. No executable Publish is qualified;
        // no External route is admitted. This is not a production board claim.
        let topology = unsafe {
            Topology::new(alloc::vec![Fabric {
                ram,
                routes: alloc::vec![]
            }])
        }
        .unwrap();
        // The instruction fixture is paired only with one-Hart QEMU. Kernel
        // fences local executable publication; SMP admission still fails below.
        #[cfg(feature = "instructions")]
        let topology = {
            if tree.cpus().count() != 1 {
                return Err(boot::BootError::Description(
                    "local instruction qualification requires one Hart",
                ));
            }
            // SAFETY: the paired QEMU invocation has one Hart, coherent normal
            // RAM and no external requester/hotplug. Local FENCE.I completes
            // instruction visibility; no remote completion is claimed.
            unsafe { topology.with_local_instructions() }
        };
        boot::qualify_memory(topology).map_err(|_| {
            boot::BootError::Description(
                "native CPU memory qualification unsupported (requires one Hart)",
            )
        })?;
        boot::start()
    });
    match result {
        Ok(()) => boot::run(),
        Err(error) => boot::fail(error),
    }
}
