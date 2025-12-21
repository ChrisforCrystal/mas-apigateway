use agw::Node;
use agw::agw_service_client::AgwServiceClient;
use tonic::transport::Channel;

pub mod agw {
    tonic::include_proto!("agw.v1");
}

pub struct AgwClient {
    client: AgwServiceClient<Channel>,
    node_id: String,
}

impl AgwClient {
    pub async fn connect(
        addr: String,
        node_id: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client = AgwServiceClient::connect(addr).await?;
        println!("Connected to Control Plane");
        Ok(Self { client, node_id })
    }

    pub async fn start_stream(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let request = tonic::Request::new(Node {
            id: self.node_id.clone(),
            region: "us-east-1".to_string(), // Placeholder
            version: "0.1.0".to_string(),
        });

        let mut stream = self.client.stream_config(request).await?.into_inner();

        println!("Config stream established. Waiting for updates...");

        while let Some(snapshot) = stream.message().await? {
            println!("Received ConfigSnapshot Version: {}", snapshot.version_id);
            // In real app, apply config here
        }

        Ok(())
    }
}
