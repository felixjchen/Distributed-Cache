use crate::ecc::client::EccClient;
use crate::ecc::get_ecc_settings;
use ecc_proto::ecc_rpc_server::{EccRpc, EccRpcServer};
use ecc_proto::{
  GetKeysReply, GetKeysRequest, GetReply, GetRequest, HeartbeatReply, HeartbeatRequest, SetReply,
  SetRequest,
};
use futures::future::join_all;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tonic::{transport::Server, Request, Response, Status};

pub mod ecc_proto {
  tonic::include_proto!("ecc_proto");
}

enum State {
  Ready,
  NotReady,
}

pub struct EccRpcService {
  id: usize,
  storage: RwLock<HashMap<String, String>>,
  servers: Vec<String>,
  h: RwLock<usize>,
  state: State,
}

impl EccRpcService {
  pub async fn new(
    id: usize,
    servers: Vec<String>,
    recover: bool,
  ) -> Result<EccRpcService, Box<dyn std::error::Error>> {
    let res = EccRpcService {
      id,
      servers,
      h: RwLock::new(0 as usize),
      state: State::Ready,
      storage: RwLock::new(HashMap::new()),
    };

    if recover {
      res.recover().await?;
    }

    Ok(res)
  }

  async fn recover(&self) -> Result<(), Box<dyn std::error::Error>> {
    // TODO: Probably want to recover while no transacations are in flight
    println!("RECOVER for {:?}", self.id);
    let (k, n, block_size, servers) = get_ecc_settings();
    let target = (self.id + 1) % servers.len();

    // Get all keys to fill
    let client = EccClient::new(k, n, block_size, servers.clone()).await;
    let keys = client.get_keys_once(servers[target].clone()).await?;

    // Get all values
    let mut storage = self.storage.write().await;
    for key in keys {
      let codeword = client.get_codeword(key.clone()).await?.unwrap();
      let parity = serde_json::to_string(&codeword[self.id]).unwrap();
      storage.insert(key, parity);
    }
    Ok(())
  }

  async fn get_cluster_status(&self) {
    let (k, n, block_size, servers) = get_ecc_settings();
    let mut client = EccClient::new(k, n, block_size, servers.clone()).await;

    let mut h = self.h.write().await;
    *h = client.send_heartbeats().await;
  }
}

#[tonic::async_trait]
impl EccRpc for Arc<EccRpcService> {
  async fn set(&self, request: Request<SetRequest>) -> Result<Response<SetReply>, Status> {
    let request = request.into_inner();
    println!("Got a set request: {:?}", request.clone());
    let key = request.key;
    let value = request.value;

    let mut storage = self.storage.write().await;
    storage.insert(key, value);

    let reply = SetReply {
      status: "success".into(),
    };

    Ok(Response::new(reply))
  }
  async fn get(&self, request: Request<GetRequest>) -> Result<Response<GetReply>, Status> {
    let request = request.into_inner();
    println!("Got a get request: {:?}", request.clone());
    let key = request.key;

    let storage = self.storage.read().await;
    let value = storage.get(&key).map(|val| val.clone());

    let reply = GetReply { value };

    Ok(Response::new(reply))
  }

  async fn get_keys(&self, _: Request<GetKeysRequest>) -> Result<Response<GetKeysReply>, Status> {
    let storage = self.storage.read().await;
    let keys = storage.keys().cloned().collect::<Vec<String>>();
    let keys = serde_json::to_string(&keys).unwrap();
    let reply = GetKeysReply { keys };
    Ok(Response::new(reply))
  }

  async fn heartbeat(
    &self,
    request: Request<HeartbeatRequest>,
  ) -> Result<Response<HeartbeatReply>, tonic::Status> {
    let state = match self.state {
      State::NotReady => "NotReady".to_string(),
      State::Ready => "Ready".to_string(),
    };
    Ok(Response::new(HeartbeatReply { state }))
  }
}

pub async fn start_server(
  id: usize,
  addr: String,
  servers: Vec<String>,
  recover: bool,
) -> Result<(), Box<dyn std::error::Error>> {
  let addr = addr.parse().unwrap();
  let service = EccRpcService::new(id, servers, recover).await?;
  println!("Starting ecc cache node at {:?}", addr);

  let service = Arc::new(service);

  let server_future = Server::builder()
    .add_service(EccRpcServer::new(service.clone()))
    .serve(addr);

  let handle = Handle::current();
  handle.spawn(async move {
    loop {
      service.get_cluster_status().await;
      sleep(Duration::from_millis(100)).await;
      println!("{:?}", service.h.read().await);
    }
  });

  server_future.await?;
  Ok(())
}

pub async fn start_many_servers(servers: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
  let mut futures = Vec::new();
  for (id, addr) in servers.clone().into_iter().enumerate() {
    let future = start_server(id, addr, servers.clone(), false);
    futures.push(future);
  }
  join_all(futures).await;

  Ok(())
}
