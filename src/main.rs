//! `cable_tunnel` shares a [Token] over a caBLE connection.
// #[macro_use]
// extern crate tracing;
use bluetooth_hci::{
    BdAddr, BdAddrType,
    host::{
        AdvertisingFilterPolicy, AdvertisingParameters, Channels, Hci, OwnAddressType,
        uart::{CommandHeader, Hci as UartHci, Packet},
    },
    types::{Advertisement, AdvertisingInterval, AdvertisingType},
};
use clap::{ArgGroup, Parser};
use futures::StreamExt;
use openssl::rand::rand_bytes;
use serialport::FlowControl;
use serialport_hci::{
    SerialController,
    vendor::none::{Event, Vendor},
};
use tokio::sync::Mutex;
use uuid::{Uuid, uuid};

use bluer::{
    Adapter, Session,
    adv::{Advertisement as BluerAdvertisement, AdvertisementHandle, Type},
};
// #[cfg(feature = "softtoken")]
use std::fs::OpenOptions;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
    time::Duration,
};
// #[cfg(feature = "cable-override-tunnel")]
// use tokio_tungstenite::tungstenite::http::{
//     Uri,
//     uri::{Builder, Parts},
// };

use webauthn_authenticator_rs::{
    cable::{Advertiser, ShareCableAuthenticatorOptions, share_cable_authenticator},
    ctap2::CtapAuthenticator,
    error::WebauthnCError,
    softtoken::SoftToken,
    transport::{AnyTransport, TokenEvent, Transport},
    ui::Cli,
};

// #[cfg(feature = "softtoken")]
use webauthn_authenticator_rs::softtoken::SoftTokenFile;

#[derive(Debug, clap::Parser)]
#[clap(about = "caBLE tunneler tool")]
#[clap(group(
    ArgGroup::new("url")
        .required(true)
        .args(&["cable_url", "qr_image"])
))]
pub struct CliParser {
    /// Serial port where Bluetooth HCI controller is connected to.
    ///
    /// This has primarily been tested using a Nordic nRF52 microcontroller
    /// running [Apache Mynewt's NimBLE HCI demo][0].
    ///
    /// [0]: https://mynewt.apache.org/latest/tutorials/ble/blehci_project.html
    #[clap(short, long)]
    pub serial_port: String,

    /// Baud rate for communication with Bluetooth HCI controller.
    ///
    /// By default, Apache Mynewt's NimBLE HCI demo runs at 1000000 baud.
    #[clap(short, long, default_value = "1000000")]
    pub baud_rate: u32,

    /// Tunnel server ID to use. 0 = Google.
    #[clap(short, long, default_value = "0")]
    pub tunnel_server_id: u16,

    /// `FIDO:/` URL from the initiator's QR code. Either this option or
    /// --qr-image is required.
    #[clap(short, long)]
    pub cable_url: Option<String>,

    /// Screenshot of the initator's QR code. Either this option or --cable-url
    /// is required.
    ///
    /// Use `adb` to screenshot an Android device with USB debugging:
    ///
    /// adb exec-out screencap -p > screenshot.png
    ///
    /// Use Xcode to screenshot an iOS device with debugging. There is no need
    /// to open an Xcode project: from the `Window` menu, select
    /// `Devices and Simulators`, then select the device, and press
    /// `Take Screenshot`. The screenshot will be saved to `~/Desktop`.
    ///
    /// Note: this *will not* work in the Android emulator or iOS simulator,
    /// because they do not have access to a physical Bluetooth controller.
    #[clap(short, long)]
    pub qr_image: Option<String>,

    #[cfg(feature = "softtoken")]
    /// Path to saved SoftToken.
    ///
    /// You can create a new SoftToken with the `softtoken` example:
    ///
    /// cargo run --example softtoken --features softtoken -- create /tmp/softtoken.dat
    ///
    /// If this option is not specified, `cable_tunnel` will attempt to connect
    /// to the first supported physical token using AnyTransport. Most initators
    /// (browsers) will *also* attempt to connect directly to *all* physical
    /// tokens, so only use this if your initator is running on another device!
    #[clap(long)]
    pub softtoken_path: Option<String>,

    #[cfg(feature = "cable-override-tunnel")]
    /// Overrides the WebSocket tunnel protocol and domain,
    /// eg: ws://localhost:8080
    ///
    /// The initiator will need the same override set, as setting this
    /// option makes the library incompatible with other caBLE implementations.
    #[clap(long)]
    pub tunnel_uri: Option<String>,
}

struct SerialHciAdvertiser {
    // hci: SerialController<CommandHeader, Vendor>,
    adapter: Adapter,
    _advert_handle: Option<AdvertisementHandle>,
}

impl SerialHciAdvertiser {
    async fn new() -> Self {
        // let port = serialport::new(serial_port, baud_rate)
        //     .timeout(Duration::from_secs(2))
        //     .flow_control(FlowControl::None)
        //     .open()
        //     .unwrap();
        // Self {
        //     hci: SerialController::new(port),
        // }
        let session = Session::new().await.expect("safas");
        let adapter = session.default_adapter().await.expect("fdssafa");
        Self {
            adapter,
            _advert_handle: None,
        }
    }

    // fn read(&mut self) -> Packet<Event> {
    //     let r = self.hci.read().unwrap();
    //     // trace!("<<< {:?}", r);
    //     r
    // }
}

impl Advertiser for SerialHciAdvertiser {
    fn stop_advertising(&mut self) -> Result<(), WebauthnCError> {
        println!("fasfas");
        if let Some(handle) = self._advert_handle.take() {
            drop(handle);
        }
        // trace!("sending reset...");
        // self.hci.reset().unwrap();
        // let _ = self.read();
        //
        // self.hci.le_set_advertise_enable(false).unwrap();
        // let _ = self.read();
        Ok(())
    }

    fn start_advertising(
        &mut self,
        service_uuid: u16,
        payload: &[u8],
    ) -> Result<(), WebauthnCError> {
        let adapter = self.adapter.clone();
        let payload = payload.to_vec();

        // This allows blocking without panicking inside the runtime
        let result = tokio::task::block_in_place(|| {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async move {
                let uuid = uuid!("0000fff9-0000-1000-8000-00805f9b34fb");

                let mut adv_data = BTreeMap::new();
                adv_data.insert(uuid, payload);

                let mut service_uuids = BTreeSet::new();
                service_uuids.insert(uuid);

                let advert = BluerAdvertisement {
                    advertisement_type: Type::Broadcast,
                    service_data: adv_data,
                    service_uuids,
                    ..Default::default()
                };

                let handle = adapter.advertise(advert).await.expect("fsaf");
                Ok::<_, WebauthnCError>(handle)
            })
        });

        result.map(|handle| {
            self._advert_handle = Some(handle);
        })?;

        Ok(())
        // let adapter = self.adapter.clone();
        // let payload = payload.to_vec();

        // let advert_task = async {
        //     let uuid = uuid!("0000fff9-0000-1000-8000-00805f9b34fb");
        //     let mut adv_data = BTreeMap::new();
        //     adv_data.insert(0xFF, payload.to_vec());
        //     let mut service_uuids = BTreeSet::new();
        //     service_uuids.insert(uuid);
        //     let advert = BluerAdvertisement {
        //         advertisement_type: Type::Broadcast,
        //         local_name: Some("MyBLEDevice".to_string()),
        //         advertising_data: adv_data,
        //         service_uuids,
        //         ..Default::default()
        //     };

        //     let handle = self.adapter.advertise(advert).await.expect("safasf");
        //     self._advert_handle = Some(handle);
        //     Ok::<(), WebauthnCError>(())
        // };

        // // Block on the async task (runs in current thread)
        // tokio::runtime::Handle::current()
        //     .block_on(advert_task)
        //     .expect("safasf");

        // Ok(())
        // let uuid = uuid!("0000fff9-0000-1000-8000-00805f9b34fb");
        // let mut adv_data: BTreeMap<u8, Vec<u8>> = BTreeMap::new();
        // adv_data.insert(0xFF, payload.to_vec());
        // let service_uuids = BTreeSet::new();
        // service_uuids.insert(uuid);
        // let advert = BluerAdvertisement {
        //     advertisement_type: Type::Broadcast,
        //     local_name: Some("MyBLEDevice".to_string()),
        //     advertising_data: adv_data,
        //     service_uuids,
        //     ..Default::default()
        // };
        // // Start advertising and keep the handle so it continues
        // let handle = self.adapter.advertise(advert).await.expect("fsdg")
        // self._advert_handle = Some(handle);
        //
        // Ok(())

        // self.stop_advertising()?;
        // let advert = Advertisement::ServiceData16BitUuid(service_uuid, payload);
        // let mut service_data = [0; 31];
        // let len = advert.copy_into_slice(&mut service_data);
        //
        // let p = AdvertisingParameters {
        //     advertising_interval: AdvertisingInterval::for_type(
        //         AdvertisingType::NonConnectableUndirected,
        //     )
        //     .with_range(Duration::from_millis(100), Duration::from_millis(500))
        //     .unwrap(),
        //     own_address_type: OwnAddressType::Random,
        //     peer_address: BdAddrType::Random(bluetooth_hci::BdAddr([0xc0; 6])),
        //     advertising_channel_map: Channels::all(),
        //     advertising_filter_policy: AdvertisingFilterPolicy::WhiteListConnectionAllowScan,
        // };
        // let mut addr = [0u8; 6];
        // addr[5] = 0xc0;
        // rand_bytes(&mut addr[..5])?;
        //
        // self.hci.le_set_random_address(BdAddr(addr)).unwrap();
        // let _ = self.read();
        //
        // self.hci.le_set_advertising_parameters(&p).unwrap();
        // let _ = self.read();
        //
        // self.hci
        //     .le_set_advertising_data(&service_data[..len])
        //     .unwrap();
        // let _ = self.read();
        //
        // self.hci.le_set_advertise_enable(true).unwrap();
        // let _ = self.read();
        // Ok(())
    }
}

#[tokio::main]
async fn main() {
    // let _ = tracing_subscriber::fmt::try_init();

    // let opt = CliParser::parse();
    let cable_url = if let Some(u) = None {
        u
    } else if let Some(img) = Some("/home/shatrugna/Pictures/Screenshots/image.png") {
        let img = image::open(img).unwrap();
        // Optimised for screenshots from the device.
        let img = img.adjust_contrast(9000.0);

        let decoder = bardecoder::default_decoder();
        let fido_url = decoder
            .decode(&img)
            .into_iter()
            .filter_map(|r| {
                // trace!(?r);
                r.ok()
            })
            .find(|u| {
                // trace!("Found QR code: {:?}", u);
                let u = u.to_ascii_uppercase();
                u.starts_with("FIDO:/")
            });
        match fido_url {
            Some(u) => u,
            None => {
                panic!("Could not find any FIDO URLs in the image");
            }
        }
    } else {
        unreachable!();
    };

    let mut advertiser = SerialHciAdvertiser::new().await;
    let ui = Cli {};
    let options = ShareCableAuthenticatorOptions::default().tunnel_server_id(0);

    // #[cfg(feature = "cable-override-tunnel")]
    // let options = if let Some(u) = None {
    //     let parts: Parts = u.parse::<Uri>().unwrap().into_parts();
    //     let builder = Builder::new()
    //         .scheme(parts.scheme.unwrap())
    //         .authority(parts.authority.unwrap());
    //
    //     options.tunnel_uri(builder)
    // } else {
    //     options
    // };

    // #[cfg(feature = "softtoken")]
    if let Some(p) = Some("/home/shatrugna/shatrugna_data/temp/test") {
        println!("Fuck");
        // Use a SoftToken
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(p)
            .unwrap();
        // let mut softtoken = SoftTokenFile::open(f).unwrap();
        let mut softtoken = SoftToken::new(false).unwrap();
        let info = softtoken.0.get_info();

        share_cable_authenticator(
            &mut softtoken.0,
            info,
            // cable_url.trim(),
            "FIDO:/114679197220210304906771648424531766782147859148281320541770968404732770424621517034548786350173915593237623463277170050068766811029772310526807692018434109321447142404".to_string().trim(),
            &mut advertiser,
            &ui,
            options,
        )
        .await
        .unwrap();
        // let s = &softtoken.0;
        // let d = toml::to_string(s).unwrap();
        // println!("{}", d);
        return;
    }

    // Use a physical authenticator
    let transport = AnyTransport::new().await.unwrap();
    let mut events = transport.watch().await.unwrap();

    let token = loop {
        match events.next().await.unwrap() {
            TokenEvent::Added(t) => {
                break t;
            }
            TokenEvent::EnumerationComplete => {
                println!("enumeration completed without detecting authenticator, connect one!");
            }
            TokenEvent::Removed(i) => {
                println!("token disconnected: {i:?}");
            }
        }
    };

    let mut authenticator = CtapAuthenticator::new(token, &ui).await.unwrap();
    let info = authenticator.get_info().to_owned();

    share_cable_authenticator(
        &mut authenticator,
        info,
        cable_url.trim(),
        &mut advertiser,
        &ui,
        options,
    )
    .await
    .unwrap();
}
