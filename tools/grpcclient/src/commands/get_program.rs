use anyhow::Result;
use rpc_types::*;
use crate::client::ProverNetworkClient;

pub async fn run_get_program(url: String, vk_hash: String) -> Result<()> {
    println!("\n=== run_get_program ===");
    
    let mut client = ProverNetworkClient::new(url).await
        .map_err(|e| anyhow::anyhow!("Failed to create client: {}", e))?;
    
    println!("\n--- Client Request ---");

    let request = GetProgramRequest {
        vk_hash: hex::decode(&vk_hash)
            .map_err(|e| anyhow::anyhow!("Invalid vk_hash hex: {}", e))?,
    };

    println!("Requesting program with VK Hash: {}", vk_hash);
    let response = client.get_program(request).await?;
    let response_inner = response.into_inner();
    
    println!("Client received program response:");
    print_get_program_response(&response_inner);
    
    println!("\n=== Client Finished ===");
    Ok(())
}

/// Pretty print the get program response with hex-encoded byte arrays
fn print_get_program_response(response: &GetProgramResponse) {
    println!("GetProgramResponse {{");
    
    if let Some(program) = &response.program {
        println!("  program: Some(Program {{");
        println!("    vk_hash: \"{}\"", hex::encode(&program.vk_hash));
        println!("    vk: \"{}\"", hex::encode(&program.vk));
        println!("    program_uri: \"{}\"", program.program_uri);
        
        if let Some(name) = &program.name {
            println!("    name: Some(\"{}\")", name);
        } else {
            println!("    name: None");
        }
        
        println!("    owner: \"{}\"", hex::encode(&program.owner));
        println!("    created_at: {}", program.created_at);
        println!("  }})");
    } else {
        println!("  program: None");
    }
    
    println!("}}");
}
