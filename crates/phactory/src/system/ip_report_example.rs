use chain::BlockNumber;
use phala_mq::Sr25519MessageChannel;

use crate::side_task::async_side_task::AsyncSideTask;
use crate::side_task::SideTaskManager;

pub fn process_block(
    block_number: BlockNumber,
    egress: &Sr25519MessageChannel,
    side_task_man: &mut SideTaskManager,
) {
    if block_number % 2 == 1 {
        let egress = egress.clone();
        let duration = 3; // Report the result after 3 blocks no matter whether received the http response.
        let task = AsyncSideTask::spawn(
            block_number,
            duration,
            async {
                // Do network request in this block and return the result.
                // Do NOT send mq message in this block.
                log::info!("Side task start to get ip");
                let mut resp = match surf::get("https://ip.kvin.wang").send().await {
                    Ok(r) => r,
                    Err(err) => {
                        return format!("Network error: {:?}", err);
                    }
                };
                let result = match resp.body_string().await {
                    Ok(body) => body,
                    Err(err) => {
                        format!("Network error: {:?}", err)
                    }
                };
                log::info!("Side task start got ip: {}", result);
                result
            },
            move |result, _context| {
                let result = result.unwrap_or("Timed out".into());
                // send mq message in this block if needed.
                log::info!("Side task reporting: {:?}", result);
                egress.sendto(&result, "some/mq/topic");
            },
        );
        side_task_man.add_task(task);
    }
}
