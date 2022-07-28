use anyhow::bail;
use skywalking::context::{span::Span, trace_context::TracingContext, tracer::Tracer};
use std::{borrow::BorrowMut, cell::RefCell, mem::take};

// TODO Support cli mode(swoole), so use dashmap to store trace context.
// static TRACING_CONTEXT_MAP: Lazy<DashMap<u64, (TracingContext, Span)>> =
// Lazy::new(|| DashMap::new());

thread_local! {
    static REQUEST_CONTEXT: std::cell::RefCell<Option<RequestContext>>  = RefCell::new(None);
}
pub struct RequestContext {
    pub tracing_context: TracingContext,
    pub entry_span: Span,
}

impl RequestContext {
    pub fn set_global(request_id: Option<u64>, ctx: Self) {
        match request_id {
            Some(_) => todo!(),
            None => {
                REQUEST_CONTEXT.with(|global_ctx| {
                    *global_ctx.borrow_mut() = Some(ctx);
                });
            }
        }
    }

    pub fn remove_global(request_id: Option<u64>) -> Option<Self> {
        match request_id {
            Some(_) => todo!(),
            None => REQUEST_CONTEXT.with(|global_ctx| take(&mut *global_ctx.borrow_mut())),
        }
    }

    pub fn with_global<T>(
        request_id: Option<u64>, f: impl FnOnce(&mut RequestContext) -> T,
    ) -> Option<T> {
        match request_id {
            Some(_) => todo!(),
            None => REQUEST_CONTEXT
                .with(|global_ctx| global_ctx.borrow_mut().as_mut().map(|ctx| f(ctx))),
        }
    }

    pub fn try_with_global_tracing_context<T>(
        request_id: Option<u64>, f: impl FnOnce(&mut TracingContext) -> T,
    ) -> anyhow::Result<T> {
        match Self::with_global(request_id, |ctx| f(&mut ctx.tracing_context)) {
            Some(ctx) => Ok(ctx),
            None => bail!("global tracing context not exists"),
        }
    }
}
