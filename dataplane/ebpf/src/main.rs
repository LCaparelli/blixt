#![no_std]
#![no_main]

use core::mem;

use aya_bpf::{
    bindings::{TC_ACT_OK, TC_ACT_PIPE, TC_ACT_SHOT},
    helpers::{bpf_redirect_neigh},
    macros::{classifier, map},
    maps::HashMap,
    programs::TcContext,
};
use aya_log_ebpf::info;
use mem::size_of;
use memoffset::offset_of;

use common::{Backend, BackendKey};

#[allow(non_upper_case_globals)]
#[allow(non_snake_case)]
#[allow(non_camel_case_types)]
#[allow(dead_code)]
mod bindings;

use bindings::{ethhdr, iphdr, udphdr};

const ETH_P_IP: u16 = 0x0800;

const IPPROTO_UDP: u8 = 17;

const ETH_HDR_LEN: usize = mem::size_of::<ethhdr>();
const IP_HDR_LEN: usize = mem::size_of::<iphdr>();

// Gives us raw pointers to a specific offset in the packet
#[inline(always)]
unsafe fn ptr_at<T>(ctx: &TcContext, offset: usize) -> Result<*mut T, i64> {
    let start = ctx.data();
    let end = ctx.data_end();
    let len = mem::size_of::<T>();

    if start + offset + len > end {
        return Err(TC_ACT_OK.into());
    }

    Ok((start + offset) as *mut T)
}

#[map(name = "BACKENDS")]
static mut BACKENDS: HashMap<BackendKey, Backend> =
    HashMap::<BackendKey, Backend>::with_max_entries(128, 0);

#[classifier(name = "tc_ingress")]
pub fn tc_ingress(ctx: TcContext) -> i32 {
    match try_tc_ingress(ctx) {
        Ok(ret) => ret,
        Err(_) => TC_ACT_SHOT,
    };

    return TC_ACT_OK;
}

fn get_backend(key: BackendKey) -> Option<&'static Backend> {
    unsafe { BACKENDS.get(&key) }
}

// Make sure ip_forwarding is enabled on the interface this it attached to
fn try_tc_ingress(ctx: TcContext) -> Result<i32, i64> {
    let h_proto = u16::from_be(
        ctx.load(offset_of!(ethhdr, h_proto))
            .map_err(|_| TC_ACT_PIPE)?,
    );

    if h_proto != ETH_P_IP {
        return Ok(TC_ACT_PIPE);
    }

    let protocol = ctx
        .load::<u8>(ETH_HDR_LEN + offset_of!(iphdr, protocol))
        .map_err(|_| TC_ACT_PIPE)?;

    if protocol != IPPROTO_UDP {
        return Ok(TC_ACT_PIPE);
    }

    let ip_hdr: *mut iphdr = unsafe { ptr_at(&ctx, ETH_HDR_LEN) }?;

    let udp_header_offset = ETH_HDR_LEN + IP_HDR_LEN;

    let udp_hdr: *mut udphdr = unsafe { ptr_at(&ctx, udp_header_offset)? };

    let key = BackendKey {
        ip: u32::from_be(unsafe { (*ip_hdr).daddr }),
        port: (u16::from_be(unsafe { (*udp_hdr).dest })) as u32,
    };

    let Backend { daddr, dport, ifindex, .. } = get_backend(key).ok_or(TC_ACT_OK)?;

    info!(
        &ctx,
        "Received a packet destined for svc ip: {:X} at port: {}",
        u32::from_be(*daddr),
        u16::from_be(unsafe { (*udp_hdr).dest })
    );

    // Update destination IP
    unsafe {
        (*ip_hdr).daddr = daddr.to_be();
    }

    if (ctx.data() + ETH_HDR_LEN + size_of::<iphdr>()) > ctx.data_end() {
        info!(&ctx, "Iphdr is out of bounds");
        return Ok(TC_ACT_OK);
    }

    ctx.l3_csum_replace(ETH_HDR_LEN + offset_of!(iphdr, check), *daddr as u64, daddr.to_be() as u64, 4)?;

    // Update destination port
    unsafe { (*udp_hdr).dest = (*dport as u16).to_be() };
    // Kernel allows UDP packet with unset checksums
    unsafe { (*udp_hdr).check = 0 };

    let action = unsafe {
        bpf_redirect_neigh(
            *ifindex as u32,
            mem::MaybeUninit::zeroed().assume_init(),
            0,
            0,
        )
    };

    info!(&ctx, "redirect action: {}", action);

    Ok(action as i32)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
