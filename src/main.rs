use std::error::Error;
use std::io::Read;

use cpal;
use cpal::traits::{DeviceTrait, HostTrait};
use futures_util::stream;
use gouth::Builder;
use tonic::{
    metadata::MetadataValue,
    Request,
    transport::{Certificate, Channel, ClientTlsConfig},
};

use google::assistant::embedded::v1alpha2::*;
use google::r#type::*;
//use dasp_signal::{self as signal, Signal};
//use dasp_signal::interpolate::Converter;

pub mod google {
    pub mod api {
        include!("api/google.api.rs");
    }

    pub mod r#type {
        include!("api/google.r#type.rs");
    }

    pub mod assistant {
        pub mod embedded {
            pub mod v1alpha2 {
                include!("api/google.assistant.embedded.v1alpha2.rs");
            }
        }
    }
}

//const FILENAME: &str = "/Users/joelwinarske/CLionProjects/assistant/resources/switch_to_channel_5_16k_mono.raw";
const FILENAME: &str = "/Users/joelwinarske/CLionProjects/assistant/resources/weather_in_mountain_view_16k_mono.raw";
const CHUNK_SIZE: usize = 512;
const DEBUG_INFO: bool = false; // Enabling significantly increases latency of response.

const ENDPOINT: &str = "https://embeddedassistant.googleapis.com";


fn audio_info() -> Result<(), Box<dyn Error>> {

    println!("Supported hosts:\n  {:?}", cpal::ALL_HOSTS);
    let available_hosts = cpal::available_hosts();
    println!("Available hosts:\n  {:?}", available_hosts);

    for host_id in available_hosts {
        println!("{}", host_id.name());
        let host = cpal::host_from_id(host_id)?;

        let default_in = host.default_input_device().map(|e| e.name().unwrap());
        let default_out = host.default_output_device().map(|e| e.name().unwrap());

        println!("  Default Input Device:\n    {:?}", default_in);
        println!("  Default Output Device:\n    {:?}", default_out);

        let devices = host.devices()?;
        println!("  Devices: ");
        for (device_index, device) in devices.enumerate() {
            println!("  {}. \"{}\"", device_index + 1, device.name()?);

            // Input configs
            if let Ok(conf) = device.default_input_config() {
                println!("    Default input stream config:\n      {:?}", conf);
            }
            let input_configs = match device.supported_input_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported input configs: {:?}", e);
                    Vec::new()
                }
            };
            if !input_configs.is_empty() {
                println!("    All supported input stream configs:");
                for (config_index, config) in input_configs.into_iter().enumerate() {
                    println!(
                        "      {}.{}. {:?}",
                        device_index + 1,
                        config_index + 1,
                        config
                    );
                }
            }

            // Output configs
            if let Ok(conf) = device.default_output_config() {
                println!("    Default output stream config:\n      {:?}", conf);
            }
            let output_configs = match device.supported_output_configs() {
                Ok(f) => f.collect(),
                Err(e) => {
                    println!("    Error getting supported output configs: {:?}", e);
                    Vec::new()
                }
            };
            if !output_configs.is_empty() {
                println!("    All supported output stream configs:");
                for (config_index, config) in output_configs.into_iter().enumerate() {
                    println!(
                        "      {}.{}. {:?}",
                        device_index + 1,
                        config_index + 1,
                        config
                    );
                }
            }
        }
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), Box<dyn Error>> {

    //
    // Audio
    //
    audio_info()?;

    //
    // Service Configuration
    //
    let token = Builder::new()
        .scopes(&["https://www.googleapis.com/auth/assistant-sdk-prototype"])
        .build()
        .unwrap();
    println!("authorization: {}", token.header_value().unwrap());

    let certs = tokio::fs::read("certs/roots.pem")
        .await?;

    let tls_config = ClientTlsConfig::new()
        .ca_certificate(Certificate::from_pem(certs.as_slice()))
        .domain_name("embeddedassistant.googleapis.com");

    let channel = Channel::from_static(ENDPOINT)
        .tls_config(tls_config).unwrap()
        .connect()
        .await?;

    let mut service = embedded_assistant_client::EmbeddedAssistantClient::with_interceptor(channel, move |mut req: Request<()>| {
        let token = &*token.header_value().unwrap();
        let meta = MetadataValue::from_str(token).unwrap();
        req.metadata_mut().insert("authorization", meta);
        Ok(req)
    });

    //
    // Config Message
    //

    let mut messages = vec![AssistRequest {
        r#type: Some(
            assist_request::Type::Config(
                AssistConfig {
                    r#type: Some(assist_config::Type::AudioInConfig(
                        AudioInConfig {
                            encoding: 1,
                            sample_rate_hertz: 16_000,
                        }
                    )),
                    audio_out_config: Some(AudioOutConfig {
                        encoding: 1,
                        sample_rate_hertz: 16_000,
                        volume_percentage: 0,
                    }),
                    dialog_state_in: Some(DialogStateIn {
                        conversation_state: vec![0], // TODO: persist prior value of returned conversation_state
                        language_code: "en-US".to_string(), // TODO: Query platform for locale
                        device_location: Some(DeviceLocation { //TODO: Query platform for Coordinates
                            r#type: Some(device_location::Type::Coordinates(
                                LatLng {
                                    latitude: 47.606209,
                                    longitude: -122.332069,
                                }
                            ))
                        }),
                        is_new_conversation: true, // TODO: Detect if new conversation
                    }),
                    device_config: Some(DeviceConfig {
                        device_id: "unknown".to_string(),
                        device_model_id: "unknown".to_string(),
                    }),
                    debug_config: { Some(DebugConfig { return_debug_info: DEBUG_INFO }) },
                    screen_out_config: { Some(ScreenOutConfig { screen_mode: 0 }) },
                }
            )
        )
    }];

    //
    // Append Audio Data
    //

    let mut file = std::fs::File::open(FILENAME)?;
    loop {
        let mut chunk = Vec::with_capacity(CHUNK_SIZE);
        let n = file.by_ref().take(CHUNK_SIZE as u64).read_to_end(&mut chunk)?;
        if n == 0 { break; }

        messages.push(AssistRequest {
            r#type: Some(
                assist_request::Type::AudioIn(
                    chunk.clone())
            )
        });

        if n < CHUNK_SIZE { break; }
    }

    //
    // Call Assist
    //

    let mut response = service
        .assist(Request::new(stream::iter(messages)))
        .await?
        .into_inner();

    //
    // Handle Response
    //

    while let Some(res) = response.message().await? {
        if res.event_type == 1 {
            println!("** End Of Utterance **");
        }
        for spr in res.speech_results.iter() {
            println!("[{}]: \"{}\"", spr.stability, spr.transcript);
        }

        match res.audio_out {
            Some(audio_out) => {
                println!("audio_data.len: {}", audio_out.audio_data.len());
            },
            _ => {}
        }

        match res.screen_out {
            Some(screen_out) => println!("screen_out.format: {}", screen_out.format),
            _ => {}
        }

        match res.device_action {
            Some(device_action) => println!("{:?}", device_action.device_request_json),
            _ => {}
        }

        match res.dialog_state_out {
            Some(dialog_state_out) => {
                if !dialog_state_out.supplemental_display_text.is_empty() {
                    println!("supplemental_display_text: {}", dialog_state_out.supplemental_display_text);
                }
                println!("conversation_state.len: {}", dialog_state_out.conversation_state.len());
                if dialog_state_out.microphone_mode == 1 {
                    println!("microphone_mode: CLOSE_MICROPHONE");
                } else if dialog_state_out.microphone_mode == 2 {
                    println!("microphone_mode: DIALOG_FOLLOW_ON");
                }
                if dialog_state_out.volume_percentage > 0 {
                    println!("** Set Volume to {}% **", dialog_state_out.volume_percentage);
                }
            }
            _ => {}
        }

        match res.debug_info {
            Some(debug_info) => println!("{:?}", debug_info.aog_agent_to_assistant_json),
            _ => {}
        };
    }

    println!("Done!");

    Ok(())
}

