use shared::ServerMsg;
use std::collections::HashMap;
use std::sync::Mutex;

/// 消息中枢：负责处理客户端专属私信队列与全服广播
#[derive(Default)]
pub struct MessageHub {
    /// 每个在线玩家待投递的消息队列 (client_id -> [ServerMsg])
    queues: Mutex<HashMap<String, Vec<ServerMsg>>>,
}

impl MessageHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册新上线的玩家会话
    pub fn register(&self, client_id: &str) {
        let mut q = self.queues.lock().unwrap();
        q.entry(client_id.to_string()).or_default();
    }

    /// 注销断开连接的玩家
    pub fn unregister(&self, client_id: &str) {
        let mut q = self.queues.lock().unwrap();
        q.remove(client_id);
    }

    /// 向特定客户端下发私信/系统回执
    pub fn send_to(&self, client_id: &str, msg: ServerMsg) {
        let mut q = self.queues.lock().unwrap();
        if let Some(list) = q.get_mut(client_id) {
            list.push(msg);
        }
    }

    /// 全服广播消息
    pub fn broadcast(&self, msg: ServerMsg) {
        let mut q = self.queues.lock().unwrap();
        for (_, list) in q.iter_mut() {
            list.push(msg.clone());
        }
    }

    /// 广播给除指定玩家之外的所有在线玩家
    pub fn broadcast_except(&self, exclude_id: &str, msg: ServerMsg) {
        let mut q = self.queues.lock().unwrap();
        for (id, list) in q.iter_mut() {
            if id != exclude_id {
                list.push(msg.clone());
            }
        }
    }

    /// 提取并清空该玩家的所有待发送消息
    pub fn drain(&self, client_id: &str) -> Vec<ServerMsg> {
        let mut q = self.queues.lock().unwrap();
        if let Some(list) = q.get_mut(client_id) {
            std::mem::take(list)
        } else {
            Vec::new()
        }
    }

    /// 获取当前在线玩家总数
    pub fn active_count(&self) -> usize {
        let q = self.queues.lock().unwrap();
        q.len()
    }

    /// 获取当前所有在线玩家 ID 列表
    pub fn active_players(&self) -> Vec<String> {
        let q = self.queues.lock().unwrap();
        q.keys().cloned().collect()
    }
}
