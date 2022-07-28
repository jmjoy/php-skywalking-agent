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
    self, IpcBytesReceiver, IpcBytesSender, IpcError, IpcReceiver, IpcSender, IpcSharedMemory,
    TryRecvError,
};
use once_cell::sync::{Lazy, OnceCell};
use phper::ini::Ini;
use skywalking::{
    context::tracer::{SegmentReceiver, SegmentSender},
    reporter::Reporter,
    skywalking_proto::v3::SegmentObject,
};
use std::{
    cell::RefCell,
    collections::LinkedList,
    error::Error,
    intrinsics::transmute,
    mem::{replace, size_of},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
};
use tokio::task;
use tonic::async_trait;
use tracing::{debug, error, info, log::Record};

const MAX_COUNT: usize = 100;

pub static MAX_LENGTH: Lazy<usize> = Lazy::new(|| {
    let mut max_length = Ini::get::<i64>(SKYWALKING_AGENT_MAX_MESSAGE_LENGTH).unwrap_or(0) as usize;
    if max_length <= 0 {
        max_length = usize::MAX;
    }
    max_length
});

thread_local! {
    static SENDER: RefCell<Option<IpcSender<SegmentObject>>> = Default::default();
}

static RECEIVER: OnceCell<Mutex<IpcReceiver<SegmentObject>>> = OnceCell::new();

pub fn init_channel() -> anyhow::Result<()> {
    get_count()?;

    let max_length = *MAX_LENGTH;
    info!(max_length, "The max length of report body");

    let channel = ipc::channel()?;

    SENDER.with(|sender| {
        if sender.borrow().is_some() {
            bail!("Channel has initialized");
        }
        *sender.borrow_mut() = Some(channel.0);
        Ok(())
    })?;

    if RECEIVER.set(Mutex::new(channel.1)).is_err() {
        bail!("Channel has initialized");
    }

    Ok(())
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

fn channel_send(data: SegmentObject) -> anyhow::Result<()> {
    // if data.len() > *MAX_LENGTH {
    //     bail!("Send data is too big");
    // }

    let old_count = get_count()?.fetch_add(1, Ordering::SeqCst);
    if old_count >= MAX_COUNT {
        bail!("Channel is fulled");
    }
    debug!("Channel remainder count: {}", old_count);

    SENDER.with(|sender| {
        sender
            .borrow_mut()
            .as_ref()
            .context("Channel haven't initialized")
            .and_then(|sender| sender.send(data).context("Channel send failed"))
    })
}

fn channel_receive() -> anyhow::Result<SegmentObject> {
    let receiver = RECEIVER
        .get()
        .context("Channel haven't initialized")?
        .lock()
        .map_err(|_| anyhow!("Get lock failed"))?;

    let r = receiver.recv();
    get_count()?.fetch_sub(1, Ordering::SeqCst);
    Ok(r?)
}

fn channel_try_receive() -> anyhow::Result<Option<SegmentObject>> {
    let receiver = RECEIVER
        .get()
        .context("Channel haven't initialized")?
        .lock()
        .map_err(|_| anyhow!("Get lock failed"))?;

    let r = match receiver.try_recv() {
        Ok(data) => Ok(Some(data)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(e) => Err(e.into()),
    };
    get_count()?.fetch_sub(1, Ordering::SeqCst);
    r
}

pub struct Sender;

impl SegmentSender for Sender {
    fn send(&self, segment: SegmentObject) -> Result<(), Box<dyn Error>> {
        Ok(channel_send(segment)?)
    }
}

pub struct Receiver;

#[async_trait]
impl SegmentReceiver for Receiver {
    async fn recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        match task::spawn_blocking(channel_receive).await {
            Ok(r) => match r {
                Ok(segment) => Ok(Some(segment)),
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(Box::new(e)),
        }
    }

    async fn try_recv(&self) -> Result<Option<SegmentObject>, Box<dyn Error + Send>> {
        match task::spawn_blocking(channel_try_receive).await {
            Ok(r) => match r {
                Ok(segment) => Ok(segment),
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(Box::new(e)),
        }
    }
}
