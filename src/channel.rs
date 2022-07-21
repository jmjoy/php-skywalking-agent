// Copyright (c) 2022 jmjoy
// Helper is licensed under Mulan PSL v2.
// You can use this software according to the terms and conditions of the Mulan
// PSL v2. You may obtain a copy of Mulan PSL v2 at:
//          http://license.coscl.org.cn/MulanPSL2
// THIS SOFTWARE IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES OF ANY
// KIND, EITHER EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// NON-INFRINGEMENT, MERCHANTABILITY OR FIT FOR A PARTICULAR PURPOSE.
// See the Mulan PSL v2 for more details.

use crate::SKYWALKING_AGENT_MAX_MESSAGE_LENGTH;
use anyhow::{anyhow, bail, Context};
use ipc_channel::ipc::{
    self, IpcBytesReceiver, IpcBytesSender, IpcReceiver, IpcSender, IpcSharedMemory,
};
use once_cell::sync::{Lazy, OnceCell};
use phper::ini::Ini;
use skywalking::{reporter::Reporter, skywalking_proto::v3::SegmentObject};
use std::{
    collections::LinkedList,
    intrinsics::transmute,
    mem::size_of,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tonic::async_trait;
use tracing::{debug, error, info};

const MAX_COUNT: usize = 100;

pub static MAX_LENGTH: Lazy<usize> = Lazy::new(|| {
    let mut max_length = Ini::get::<i64>(SKYWALKING_AGENT_MAX_MESSAGE_LENGTH).unwrap_or(0) as usize;
    if max_length <= 0 {
        max_length = usize::MAX;
    }
    max_length
});

static SENDER: OnceCell<Mutex<IpcSender<LinkedList<SegmentObject>>>> = OnceCell::new();
static RECEIVER: OnceCell<Mutex<IpcReceiver<LinkedList<SegmentObject>>>> = OnceCell::new();

pub fn init_channel() -> anyhow::Result<()> {
    get_count()?;

    let max_length = *MAX_LENGTH;
    info!(max_length, "The max length of report body");

    let channel = ipc::channel()?;

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

pub fn channel_send(data: LinkedList<SegmentObject>) -> anyhow::Result<()> {
    if data.len() > *MAX_LENGTH {
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

pub fn channel_receive() -> anyhow::Result<LinkedList<SegmentObject>> {
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

pub struct IpcReporter;

#[async_trait]
impl Reporter for IpcReporter {
    async fn collect(&mut self, segments: LinkedList<SegmentObject>) -> skywalking::Result<()> {
        if let Err(e) = channel_send(segments) {
            error!(error = ?e, "Collect");
        }
        Ok(())
    }
}
