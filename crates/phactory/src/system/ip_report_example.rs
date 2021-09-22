use chain::BlockNumber;
use phala_mq::Sr25519MessageChannel;
use std::sync::{Arc, Mutex};

use crate::side_task::{PollContext, PollState, SideTask, SideTaskManager};

struct IpReportSideTask<T> {
    report_at: BlockNumber,
    result: Arc<Mutex<Option<String>>>,
    egress: Sr25519MessageChannel,
    _async_task: T,
}

impl<T: Send> SideTask for IpReportSideTask<T> {
    fn poll(self, block: &PollContext) -> PollState<Self> {
        if block.block_number >= self.report_at {
            let message = self.result.lock().unwrap().take().unwrap_or("Timed out".into());
            log::info!("Reporting side task result: {:?}", message);
            self.egress.sendto(&message, "phala/example/topic");
            PollState::Complete
        } else {
            PollState::Running(self)
        }
    }
}

pub fn process_block(
    block_number: BlockNumber,
    egress: &Sr25519MessageChannel,
    side_task_man: &mut SideTaskManager,
) {
    if block_number % 2 == 1 {
        let result = Arc::new(Mutex::new(None));
        let set_result = result.clone();
        // Spawn an async task to do the HTTP request.
        log::info!("Spawn side task start to get ip");
        let async_task = phala_async_executor::spawn(async move {
            log::info!("Side task start to get ip");
            let mut resp = match surf::get("https://ip.kvin.wang").send().await {
                Ok(r) => r,
                Err(err) => {
                    *set_result.lock().unwrap() = Some(format!("Network error: {:?}", err));
                    return;
                }
            };
            let result = match resp.body_string().await {
                Ok(body) => body,
                Err(err) => {
                    format!("Network error: {:?}", err)
                }
            };
            log::info!("Side task start got ip: {}", result);
            *set_result.lock().unwrap() = Some(result);
        });

        let task = IpReportSideTask {
            report_at: block_number + 2,
            result,
            egress: egress.clone(),
            _async_task: async_task,
        };

        side_task_man.add_task(task);
    }
}
