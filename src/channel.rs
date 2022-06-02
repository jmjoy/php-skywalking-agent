use crate::SKYWALKING_AGENT_MAX_MESSAGE_LENGTH;
use anyhow::{anyhow, bail, Context};
use crossbeam_utils::atomic::AtomicCell;
use ipc_channel::ipc::{
    self, IpcBytesReceiver, IpcBytesSender, IpcReceiver, IpcSender, IpcSharedMemory,
};
use once_cell::sync::{Lazy, OnceCell};
use phper::ini::Ini;
use skywalking_rust::skywalking_proto::v3::SegmentObject;
use std::{
    cell::RefCell,
    intrinsics::transmute,
    mem::size_of,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tracing::debug;

const MAX_COUNT: usize = 100;

static MAX_LENGTH: AtomicCell<usize> = AtomicCell::new(0);

static SENDER: OnceCell<Mutex<IpcBytesSender>> = OnceCell::new();
static RECEIVER: OnceCell<Mutex<IpcBytesReceiver>> = OnceCell::new();

pub fn init_channel() -> anyhow::Result<()> {
    get_count()?;

    let mut max_length = Ini::get::<i64>(SKYWALKING_AGENT_MAX_MESSAGE_LENGTH).unwrap_or(0) as usize;
    if max_length <= 0 {
        max_length = usize::MAX;
    }
    MAX_LENGTH.store(max_length);

    let channel = ipc::bytes_channel()?;
    let result = SENDER.set(Mutex::new(channel.0));
    result.map_err(|_| anyhow!("Channel has initialized"))?;
    let result = RECEIVER.set(Mutex::new(channel.1));
    result.map_err(|_| anyhow!("Channel has initialized"))
}

fn get_count() -> anyhow::Result<&'static AtomicUsize> {
    static COUNT: OnceCell<IpcSharedMemory> = OnceCell::new();
    let count = COUNT.get_or_init(|| {
        let count: [u8; size_of::<AtomicUsize>()] = unsafe { transmute(AtomicUsize::new(0)) };
        IpcSharedMemory::from_bytes(&count)
    });
    let ptr = count.as_ptr() as *const AtomicUsize;
    unsafe {
        ptr.as_ref()
            .context("Shared memory of message count is null")
    }
}

pub fn channel_send(data: &[u8]) -> anyhow::Result<()> {
    if data.len() > MAX_LENGTH.load() {
        bail!("Send data is too big");
    }

    let old_count = get_count()?.fetch_add(1, Ordering::SeqCst);
    if old_count >= MAX_COUNT {
        bail!("Channel is fulled");
    }
    debug!("Channel remainder count: {}", old_count);

    SENDER
        .get()
        .context("Channel haven't initialized")?
        .lock()
        .map_err(|_| anyhow!("Get lock failed"))?
        .send(data)
        .context("Channel send failed")
}

pub fn channel_receive() -> anyhow::Result<Vec<u8>> {
    let data = RECEIVER
        .get()
        .context("Channel haven't initialized")?
        .lock()
        .map_err(|_| anyhow!("Get lock failed"))?
        .recv()
        .context("Channel send failed")?;

    get_count()?.fetch_sub(1, Ordering::SeqCst);

    Ok(data)
}
