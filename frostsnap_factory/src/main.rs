use aes::cipher::BlockEncryptMut as _;
use clap::Parser;
use core::fmt;
use frostsnap_comms::factory::{
    DeviceFactorySend, Esp32DsKey, FactoryDownstream, FactorySend, GenuineCheckKey, REPRODUCING_TEST_VECTORS,
};
use frostsnap_comms::{ReceiveSerial, MAGIC_BYTES_PERIOD};
use frostsnap_coordinator::{DesktopSerial, FramedSerialPort, Serial};
use frostsnap_core::schnorr_fun::fun::marker::{NonZero, Public, Secret};
use frostsnap_core::schnorr_fun::fun::{hex, KeyPair, Scalar};
use frostsnap_core::schnorr_fun::Message;
use hmac::{Hmac, Mac};
use num_bigint::BigUint;
use num_bigint_dig as num_bigint;
use num_traits::identities::{One, Zero};
use num_traits::ToPrimitive;
use rand::RngCore as _;
use rsa::traits::PublicKeyParts as _;
use rsa::{pkcs8::DecodePrivateKey, traits::PrivateKeyParts as _, RsaPrivateKey, RsaPublicKey};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use tracing::*;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // /// Name of the person to greet
    // #[arg(short, long)]
    // name: String,

    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // count: u8,
}

const USB_VID: u16 = 12346;
const USB_PID: u16 = 4097;
#[allow(unused)]
const ETS_DS_MAX_BITS: usize = 3072;
const SOC_RSA_MAX_BIT_LEN: usize = ETS_DS_MAX_BITS;
#[allow(unused)]
const ESP_DS_SIGNATURE_MAX_BIT_LEN: usize = SOC_RSA_MAX_BIT_LEN;
#[allow(unused)]
const ETS_DS_C_LEN: usize = (ETS_DS_MAX_BITS * 3 / 8) + 32 + 8 + 8;
const DS_MAX_WORDS: usize = ETS_DS_MAX_BITS / 32;

const FACTORY_KEY: [u8; 32] = [
    0x02, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
];

const TESTING_RSA_KEY : &str = "308206fe020100300d06092a864886f70d0101010500048206e8308206e40201000282018100b54f19ed638645a102068fb9b9a73312bd98692fcb0bd2e197f350fb427f46d6ea4ada1f585ff250564d8aca4b4efcfec9d8996b893b09bee8427ece2af1c47c9e9b8c827503c276c63e59dfc455f9fcee8c286afae480d666b2571b6c04af586a7355f43787665495389b97071e83d21e9273f9aba533d99512043e107f7cc2e148646dc7370572252cce4477951a90b7eafc5bcfc2967c3efc675168e40abcec6f495f7b3061315604dcea89b99bd9e1f7fd90c1701311eb37769d554042a12eaf620740da90e635407440fb8a2ce15919c5080f309a6edc88a0785bd8e60cb9642af2bbc740cbcdce8b3af183f327d8cd20126fe0812d73fd90ff0b990ba81ed2c88cd53b88b08c64f04a2988768d3ed7527f10ce63edfdb9c3ee64d1d6fdcc8e703a6dbbf653d40965a975c6b350b07092246b0e8954ff3e421b78ceb866898154e23628c35abd51abc0ceea02e00d79e7ab8107c14cc113d065758c9cc1d684080282be9c630d8666cf6cf7ac387238c4e15d3e131a53aecd90712b9b0b0203010001028201800eac51fd02c1729acc570099b7707f1dcf51e982b0818aa059939ba45750da9da3af741b4ba3d4309e93699de3de06c9389cfabeed07bb4b03c6a1e18885dc1b791b3de75995f8ce73a3c727e8b3f69125886ff04726bd55dca6268e35bde3fceea72fe272fe0110eba9fda4343375829dee71f6ace6a7f2be1c365359881fc386723d37c9d810ecedc799ad33d4fc25e5cf32d0dc0308fa66d48c28abfef7442bdef91c023f10ebea64bc063a7d8178997a3594af504fd2c840f5acacf7658ce7f78a087a8624428c196e28ee2d10f7872866c609fae8d0e162c4c536243ff36ddfe105186c79365964cf964a2a374ab41f72a3fb05510a8862e6e93158668ac51654ff97b8fb5b629ace66020a54c0d985f3066e0c1940865af9bf84aa231be0f74bc28303a960cc78cb09c55fb2af6654e64581214241f743492c546dc624acbaf46df0923971b058a5f33fd76427e4926a382e17e723437784f899fcf616d848c7f62d6202a7e0b6e1e7c86601db4357344ff3b9744269c3f1af90eabc910281c100da7642ebc740bef3fff065f14f63d594a9a118d894a7a2cc80d175f7062eb94bba6e569ae88724d3a351269680bb4b788f1d3c63f122e3efff6756f015dc506c94b6704aac8fa5abceedb94ecaf9e2dbe21c20202e7ff084df71d57dd6cdff5f89f7d90586e24d7e4dd37de66a6d0387e9296fb546e4b79a555d472b3516bbc2f96973e5ad14312134cfb92d5421239008a0ea662d98ee45e29a0616473f7e73022080429592bd3aaaf97b2a174feeecc74a755c2fb9c3db56d0a9c175a1cddb0281c100d4768c9dbeac5327df307e2d39ce32d86b9c85ade8e87301b74d8ce3eea6177feeb41c6c48fa72059a6bd749a362277596badd8e1fd13f72d16c67bb02e2aec57f1199e9c787d1c91bac6fdf9e69f3c68dde618ca5499e2313e540abb9ea1420cd75c219e4259eeee76236c388005566501fdb367b07fd51c9584461135bb4796653fac83f07e88dee1c026dbeb292b71b303be273b622bb51712b68986bd41b57ca203e41b5c0a46ff6d41b92f2c63431405d7a591f5e3509181e713e3ea6910281c100d737183854e418fa21927fab598dbd9426043988ebf1b5b507d6d202d849616c142ead0d10b44a7860750ab1cc0237987e4ccbf89d4ec504e334b7f5ef634aab9d5999884735807da06e9b56df298bef1872a2c77167c2d7f3949e40c943c92822b05351598f49ce7af73619af90d3a0a9f793401fa624a65b2078833d5ab7009e5adfbd4d640dfe6b9b940eeec972d26b5db36d93d00c3436c78be598ad19724d8f1d2bfb54432d2fd075208334d0e8dc7022ebfd6c61618cc625e61b6f9a6f0281c1008fbb638593e8a098e8b4b5a782e3ac221d2ad684c07c00d1b8600e6064a2986343e935114c8da17588f24bc2d575219cbb4bcf76c6af986ce4a0a1cc32378864b38204cdd2de5f5dde0ad9e43e170f83d3960e08480975a1e563c24c6a89a0f4500aca3519d319a225869be5cbabee1a393a53e29778e036e42f8292e9b5b0723077bfc09863913ff3459f9efed36fcdcfe6e19c610b6693b2950cf8c5a4ace9928a7b25a2ee8254bc2a0f745805457129a0919ca38e44fd3c19c4fe774d8b010281c0234c8c6b35da1e4111234ec11f8485902e09ff199511eedf3c8ccfd800552811d795f8014c9ae2253534cb50f563f66de07a68fbc14eba9d82123e706623e26e50a48848c066ef7110ef43e757941701eab825033754883267a4dabb628af4069d8cd6d423ec763f54f26baac010315808e654c495d6c694a2b33936250d7407d37f835e12676b6cbffbc97fda2409f2b243348bbcaf21120d1310184731aa61609b2ee181b7379e4e920dd9e7bf7007b2ba01e89466701145268a36c80515b3";
const TESTING_KEY: [u8; 32] = [
    0x54, 0xde, 0x64, 0x8e, 0xcd, 0x6a, 0x3e, 0x0e, 0xd3, 0xc5, 0x99, 0x5b, 0xdb, 0xdf, 0xd0, 0xc5,
    0xf7, 0x44, 0x3f, 0x24, 0xdd, 0xca, 0x01, 0x7d, 0x36, 0xef, 0x68, 0x21, 0x75, 0xd6, 0x4d, 0x91,
];

fn main() -> ! {
    // Initialize the subscriber with pretty formatting.
    tracing_subscriber::fmt()
        .pretty() // Enables pretty formatting
        .init();
    // let args = Args::parse();

    let serial = DesktopSerial::default();

    let mut connection_state = HashMap::new();
    loop {
        for port_desc in serial.available_ports() {
            if !connection_state.contains_key(&port_desc.id) {
                if port_desc.vid == USB_VID && port_desc.pid == USB_PID {
                    let port = serial
                        .open_device_port(&port_desc.id, 14_000 /* doesn't matter*/)
                        .map(FramedSerialPort::<FactoryDownstream>::new);

                    match port {
                        Ok(port) => {
                            connection_state.insert(
                                port_desc.id,
                                Connection {
                                    state: ConnectionState::WaitingForMagic { last_wrote: None },
                                    port,
                                },
                            );
                        }
                        Err(e) => {
                            error!(
                                port = port_desc.id.to_string(),
                                error = e.to_string(),
                                "unable to open port"
                            );
                        }
                    }
                }
            }
        }

        for (port_id, connection) in connection_state.iter_mut() {
            info_span!("polling port", port = port_id.to_string()).in_scope(|| {
                match connection.state {
                    ConnectionState::WaitingForMagic { last_wrote } => {
                        match connection.port.read_for_magic_bytes() {
                            Ok(supported_features) => match supported_features {
                                Some(_) => {
                                    connection.state = ConnectionState::Connected;
                                }
                                None => {
                                    if last_wrote.is_none()
                                        || last_wrote.as_ref().unwrap().elapsed().as_millis() as u64
                                            > MAGIC_BYTES_PERIOD
                                    {
                                        connection.state = ConnectionState::WaitingForMagic {
                                            last_wrote: Some(std::time::Instant::now()),
                                        };
                                        let _ =
                                            connection.port.write_magic_bytes().inspect_err(|e| {
                                                error!(
                                                    error = e.to_string(),
                                                    "failed to write magic bytes"
                                                );
                                            });
                                    }
                                }
                            },
                            Err(e) => {
                                error!(error = e.to_string(), "failed to read magic bytes")
                            }
                        }
                    }
                    ConnectionState::Connected => {
                        let mut bytes = [0u8; 32];
                        rand::thread_rng().fill_bytes(&mut bytes);
                        connection
                            .port
                            .raw_send(ReceiveSerial::Message(FactorySend::InitEntropy(bytes)))
                            .unwrap();
                        connection.state = ConnectionState::InitEntropy;
                    }
                    ConnectionState::InitEntropy => {
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::InitEntropyOk)) =
                            connection.port.try_read_message().unwrap()
                        {
                            let ds_key = if REPRODUCING_TEST_VECTORS {
                                ds_key_tests()
                            } else {
                                generate_ds_key() 
                            };
                   
                            connection
                                .port
                                .raw_send(ReceiveSerial::Message(FactorySend::SetEsp32DsKey(
                                    ds_key,
                                )))
                                .unwrap();

                            connection.state = ConnectionState::SettingDsKey;
                        }
                    }
                    // todo verify signature
                    ConnectionState::SettingDsKey => {
                        if let Some(ReceiveSerial::Message(DeviceFactorySend::SetDs {
                            signature,
                        })) = connection.port.try_read_message().unwrap()
                        {
                            // TODO: With random RSA keys, we will need to remember the public keys?
                            // Or have the device send them back..
                            let rsa_pcks8 = hex::decode(TESTING_RSA_KEY).unwrap();
                            let priv_key = RsaPrivateKey::from_pkcs8_der(&rsa_pcks8).unwrap();
                            let pub_key = priv_key.to_public_key();
                            let padding = rsa::Pkcs1v15Sign::new::<Sha256>();
                            let challenge = frostsnap_core::hex::decode("354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b").unwrap();

                            let verified =
                                pub_key.verify(padding, &challenge, &signature).is_ok();
                            println!("DS signature verification: {}.\nThis signature fails under normal ESP32 signing.\nI think we need to do a modified ds.rs for raw exponent sign for a standard rsa signature.", verified);

                            // create genuine certificate
                            let genuine_certificate = generate_genuine_key();
                            connection
                                .port
                                .raw_send(ReceiveSerial::Message(
                                    FactorySend::SetGenuineCertificate(genuine_certificate),
                                ))
                                .unwrap();
                            connection.state = ConnectionState::SavingGenuineCertificate;
                        }
                    }
                    ConnectionState::SavingGenuineCertificate => {
                        if let Some(ReceiveSerial::Message(
                            DeviceFactorySend::SavedGenuineCertificate,
                        )) = connection.port.try_read_message().unwrap()
                        {
                            //
                        }
                    }
                }
            });
        }
    }
}

fn raw_exponent_rsa_sign(padded_int: BigUint, private_key: &RsaPrivateKey) -> BigUint {
    let d = BigUint::from_bytes_be(&private_key.d().to_bytes_be());
    let n = BigUint::from_bytes_be(&private_key.n().to_bytes_be());
    let signature_int = padded_int.modpow(&d, &n);

    signature_int
}

fn esp_32_test_vec_sign(message: &[u8], private_key: &RsaPrivateKey) -> BigUint {
    let message_int = BigUint::from_bytes_be(message);

    let mask = (BigUint::from(1u32) << ETS_DS_MAX_BITS) - BigUint::from(1u32);
    let masked_message = message_int & mask;
    let sig = raw_exponent_rsa_sign(masked_message, private_key);
    sig
}

fn pad_message_for_rsa(message_digest: &[u8], private_key: &RsaPrivateKey) -> BigUint {
    let padding = rsa::Pkcs1v15Sign::new::<sha2::Sha256>();

    // manually apply padding
    let key_size = private_key.size();
    let mut padded_block = vec![0; key_size];

    // PKCS#1 v1.5 format: 0x00 || 0x01 || PS || 0x00 || T
    padded_block[0] = 0x00;
    padded_block[1] = 0x01;

    // Get the prefix (ASN.1 DigestInfo)
    let prefix = padding.prefix.as_ref();

    // Calculate padding length
    let padding_len = key_size - prefix.len() - message_digest.len() - 3;

    // Fill with 0xFF bytes
    for i in 0..padding_len {
        padded_block[2 + i] = 0xFF;
    }

    // Add 0x00 separator
    padded_block[2 + padding_len] = 0x00;

    // Add prefix (ASN.1 DigestInfo)
    let prefix_offset = 3 + padding_len;
    padded_block[prefix_offset..(prefix_offset + prefix.len())].copy_from_slice(prefix);

    // add message digest
    let digest_offset = prefix_offset + prefix.len();
    padded_block[digest_offset..(digest_offset + message_digest.len())]
        .copy_from_slice(&message_digest);

    BigUint::from_bytes_be(&padded_block)
}

fn generate_genuine_key() -> GenuineCheckKey {
    let genuine_key = TESTING_KEY;

    let certificate = {
        let factory_secret = Scalar::<Secret, NonZero>::from_bytes(FACTORY_KEY).unwrap();
        let factory_keypair = KeyPair::new_xonly(factory_secret);
        let schnorr = frostsnap_core::schnorr_fun::new_with_synthetic_nonces::<
            sha2::Sha256,
            rand::rngs::ThreadRng,
        >();
        let message = Message::<Public>::plain("frostsnap-genuine-key", &TESTING_KEY);
        schnorr.sign(&factory_keypair, message)
    };

    GenuineCheckKey {
        genuine_key,
        certificate,
    }
}

fn generate_ds_key() -> Esp32DsKey {
    let rsa_pcks8 = hex::decode(TESTING_RSA_KEY).unwrap();
    let priv_key = RsaPrivateKey::from_pkcs8_der(&rsa_pcks8).unwrap();

    let hmac_key = TESTING_KEY;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(&hmac_key[..]).expect("HMAC can take key of any size");
    mac.update([0xffu8; 32].as_slice());
    let aes_key: [u8; 32] = mac.finalize().into_bytes().into();
    let iv = [
        0xb8, 0xb4, 0x69, 0x18, 0x28, 0xa3, 0x91, 0xd9, 0xd6, 0x62, 0x85, 0x8c, 0xc9, 0x79, 0x48,
        0x86,
    ];

    let plaintext_data = EspDsPData::new(&priv_key).unwrap();

    let encrypted_params =
        encrypt_private_key_material(&plaintext_data, &aes_key[..], &iv[..]).unwrap();

    let challenge = frostsnap_core::hex::decode("354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b").unwrap();

    Esp32DsKey {
        encrypted_params,
        hmac_key,
        challenge,
    }
}

fn ds_key_tests() -> Esp32DsKey {
    let rsa_pcks8 = hex::decode(&TESTING_RSA_KEY).unwrap();
    let priv_key = RsaPrivateKey::from_pkcs8_der(&rsa_pcks8).unwrap();

    println!("{:?}", priv_key);
    let pub_key = RsaPublicKey::from(&priv_key);
    // println!(
    //     "{}",
    //     pub_key.to_public_key_pem(LineEnding::default()).unwrap()
    // );
    let hmac_key = TESTING_KEY;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(&hmac_key[..]).expect("HMAC can take key of any size");
    mac.update([0xffu8; 32].as_slice());
    let aes_key: [u8; 32] = mac.finalize().into_bytes().into();
    let iv = [
        0xb8, 0xb4, 0x69, 0x18, 0x28, 0xa3, 0x91, 0xd9, 0xd6, 0x62, 0x85, 0x8c, 0xc9, 0x79, 0x48,
        0x86,
    ];

    // SIGNING TESTS
    let message = hex::decode("354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b").unwrap();

    // // Reproduce ESP32 Test Vectors
    let sig = esp_32_test_vec_sign(&message, &priv_key);
    let sig_vec = big_number_to_words(&sig);
    let sig_arr = vec_to_fixed(&sig_vec, DS_MAX_WORDS);
    println!("Signature (esp32 test vector format):");
    for &word in &sig_arr {
        print!("0x{:08x}, ", word);
    }
    println!("\n");

    println!(
        "Signature hex big endian:\n{}\n",
        hex::encode(&sig.to_bytes_be())
    );

    // Normal RSA signing
    let message_digest = sha2::Sha256::digest(message);
    let padded_message = pad_message_for_rsa(&message_digest, &priv_key);
    let sig = raw_exponent_rsa_sign(padded_message, &priv_key);
    let standard_signature = sig.to_bytes_be();
    println!(
        "Standard-compliant signature (hex):\n{}",
        hex::encode(&standard_signature)
    );

    // Verify
    let padding = rsa::Pkcs1v15Sign::new::<Sha256>();
    let verified = pub_key
        .verify(padding, &message_digest, &standard_signature)
        .is_ok();
    println!("Standard signature verification: {}", verified);

    // FINISH SIGNING TESTS

    let plaintext_data = EspDsPData::new(&priv_key).unwrap();

    let encrypted_params =
        encrypt_private_key_material(&plaintext_data, &aes_key[..], &iv[..]).unwrap();

    let challenge = frostsnap_core::hex::decode("354691f19b05c1da1571ea69fa0b4874d699a89cd525d6a5a8f6a43129fd7ee0590098518560268da96aeee6e34c73e608e8d4b71ffa0b0fabd72b065dc154633d6b2a19670b983b0f6b8bebc4f88b9d42aa0618ac161f2f3f5706330c0c118e31249d95298faf8fd54950b77020df103eb192a3f9a4318b551311d3633b86cf661c3cd5d78157560d9260a87e96e705d16cfaa259d2e4b9a5dea9c7fef18bb2dc66f273f403bbecda974617bf2fa69ba4b394af904720bbf8a76a648f476e49dcc7aa885bfeae7ad79aaf6311d6535ab4191a9aeb5ee28e3c500433c7814ab24711dab2482b9991cf7c8977e7566df834fab9921f94c1b08a3c1473487fd73add0029febdeb1045c94d538b53ab1a4c7c81de0352b33d96fded278e966c0272d4f97f6e1050ce446e3a2edca4a7c0089c0476e01c6988eea643f03a3009944d9184e04f3b521e0f210ee09543387645eaa8809164ede54f959055611a74f6cd9d7eeef7884c30bd7891a82a93ebe946282309589110e3d77f217bec62ffe23b").unwrap();

    Esp32DsKey {
        encrypted_params,
        hmac_key,
        challenge,
    }
}

struct Connection {
    state: ConnectionState,
    port: FramedSerialPort<FactoryDownstream>,
}
enum ConnectionState {
    WaitingForMagic {
        last_wrote: Option<std::time::Instant>,
    },
    Connected,
    InitEntropy,
    SettingDsKey,
    SavingGenuineCertificate,
}

// Assuming a fixed key size in bits (e.g., 3072).
const RSA_KEY_SIZE: usize = 3072;

#[repr(C)]
pub struct EspDsPData {
    pub y: [u32; DS_MAX_WORDS],  // RSA exponent (private exponent)
    pub m: [u32; DS_MAX_WORDS],  // RSA modulus
    pub rb: [u32; DS_MAX_WORDS], // Montgomery R inverse operand: (1 << (RSA_KEY_SIZE*2)) % M
    pub m_prime: u32,            // - modinv(M mod 2^32, 2^32) mod 2^32
    pub length: u32,             // effective length: (RSA_KEY_SIZE/32) - 1
}

impl EspDsPData {
    /// Constructs EspDsPData from an RSA private key.
    ///
    /// Uses the following Python formulas:
    ///
    ///   rr = 1 << (key_size * 2)
    ///   rinv = rr % pub_numbers.n
    ///   mprime = - modinv(M, 1 << 32) & 0xFFFFFFFF
    ///   length = key_size // 32 - 1
    ///
    /// In this implementation we assume RSA_KEY_SIZE is the intended bit size.
    /// Y is taken as the private exponent and M as the modulus.
    pub fn new(rsa_private: &RsaPrivateKey) -> Result<Self, Box<dyn Error>> {
        // Get the private exponent (d) and modulus (n) as BigUint.
        let y_big = rsa_private.d();
        let m_big = rsa_private.n();

        // Convert Y and M into vectors of u32 words (little-endian).
        let y_vec = big_number_to_words(y_big);
        let m_vec = big_number_to_words(m_big);

        // Use the fixed RSA_KEY_SIZE to compute the effective length.
        // For example, if RSA_KEY_SIZE is 3072 then length = 3072/32 - 1 = 96 - 1 = 95.
        let length = (RSA_KEY_SIZE / 32 - 1) as u32;

        // Convert the vectors into fixed-length arrays.
        let y_arr = vec_to_fixed(&y_vec, DS_MAX_WORDS);
        let m_arr = vec_to_fixed(&m_vec, DS_MAX_WORDS);

        // Compute m_prime = - modinv(M mod 2^32, 2^32) & 0xFFFFFFFF.
        let n0 = (m_big & BigUint::from(0xffffffffu32))
            .to_u32()
            .ok_or("Failed to convert modulus remainder to u32")?;
        let inv_n0 = modinv_u32(n0).ok_or("Failed to compute modular inverse for m_prime")?;
        let m_prime = (!inv_n0).wrapping_add(1);

        // Compute Montgomery value as per Python:
        // rr = 1 << (RSA_KEY_SIZE * 2)
        // rb = rr % M
        let rr = BigUint::one() << (RSA_KEY_SIZE * 2);
        let rb_big = &rr % m_big;
        let rb_vec = big_number_to_words(&rb_big);
        let rb_arr = vec_to_fixed(&rb_vec, DS_MAX_WORDS);

        Ok(EspDsPData {
            y: y_arr,
            m: m_arr,
            rb: rb_arr,
            m_prime,
            length,
        })
    }
}

/// Converts a BigUint into a Vec<u32> in little-endian order,
/// stopping when the number becomes zero.
fn big_number_to_words(num: &BigUint) -> Vec<u32> {
    let mut vec = Vec::new();
    let mut n = num.clone();
    let mask = BigUint::from(0xffffffffu32);
    while n > BigUint::zero() {
        let word = (&n & &mask).to_u32().unwrap();
        vec.push(word);
        n >>= 32;
    }
    if vec.is_empty() {
        vec.push(0);
    }
    vec
}

/// Copies a vector of u32 into a fixed-length array, padding with zeros.
fn vec_to_fixed(vec: &Vec<u32>, fixed_len: usize) -> [u32; DS_MAX_WORDS] {
    let mut arr = [0u32; DS_MAX_WORDS];
    for (i, &word) in vec.iter().enumerate().take(fixed_len) {
        arr[i] = word;
    }
    arr
}

/// Computes the modular inverse of a modulo 2^32, assuming a is odd.
fn modinv_u32(a: u32) -> Option<u32> {
    let modulus: i64 = 1i64 << 32;
    let mut r: i64 = modulus;
    let mut new_r: i64 = a as i64;
    let mut t: i64 = 0;
    let mut new_t: i64 = 1;

    while new_r != 0 {
        let quotient = r / new_r;
        let temp_t = t - quotient * new_t;
        t = new_t;
        new_t = temp_t;
        let temp_r = r - quotient * new_r;
        r = new_r;
        new_r = temp_r;
    }
    if r > 1 {
        return None;
    }
    if t < 0 {
        t += modulus;
    }
    Some(t as u32)
}

/// Custom Debug implementation that prints u32 arrays in "0x%08x" format.
impl fmt::Debug for EspDsPData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn format_array(arr: &[u32; DS_MAX_WORDS]) -> String {
            let formatted: Vec<String> = arr.iter().map(|word| format!("0x{:08x}", word)).collect();
            format!("{{ {} }}", formatted.join(", "))
        }

        writeln!(f, "EspDsPData {{")?;
        writeln!(f, "    y: {}", format_array(&self.y))?;
        writeln!(f, "    m: {}", format_array(&self.m))?;
        writeln!(f, "    rb: {}", format_array(&self.rb))?;
        writeln!(f, "    m_prime: 0x{:08x}", self.m_prime)?;
        writeln!(f, "    length: {}", self.length)?;
        write!(f, "}}")
    }
}

use aes::cipher::{block_padding::NoPadding, KeyIvInit};
use aes::Aes256;
use cbc::Encryptor;
/// Encrypts the private key material following the ESP32-C3 DS scheme without extra padding.
///
/// It constructs:
///
///   md_in = number_as_bytes(Y, max_key_size)
///         || number_as_bytes(M, max_key_size)
///         || number_as_bytes(Rb, max_key_size)
///         || pack::<LittleEndian>(m_prime, length)
///         || iv
///
///   md = SHA256(md_in)
///
///   p = number_as_bytes(Y, max_key_size)
///         || number_as_bytes(M, max_key_size)
///         || number_as_bytes(Rb, max_key_size)
///         || md
///         || pack::<LittleEndian>(m_prime, length)
///         || [0x08; 8]
///
/// where max_key_size = RSA_KEY_SIZE/8. Then p is encrypted using AES-256 in CBC mode with no padding.
/// (Note: p must be block-aligned; for example, for a 3072-bit key, p ends up being 1200 bytes, which is
/// a multiple of 16.)
pub fn encrypt_private_key_material(
    ds_data: &EspDsPData,
    aes_key: &[u8],
    iv: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    // For a fixed RSA key size (e.g., 3072 bits), max_key_size is:
    let max_key_size = RSA_KEY_SIZE / 8; // e.g., 3072/8 = 384 bytes

    // Convert each of Y, M, and Rb into fixed-length little-endian byte arrays.
    let y_bytes = number_as_bytes(&ds_data.y, max_key_size);
    let m_bytes = number_as_bytes(&ds_data.m, max_key_size);
    let rb_bytes = number_as_bytes(&ds_data.rb, max_key_size);

    // Pack m_prime and length as little-endian u32 values.
    let mut mprime_length = Vec::new();
    mprime_length.extend_from_slice(&ds_data.m_prime.to_le_bytes());
    mprime_length.extend_from_slice(&ds_data.length.to_le_bytes());

    // Construct md_in = Y || M || Rb || (m_prime||length) || IV.
    let mut md_in = Vec::new();
    md_in.extend_from_slice(&y_bytes);
    md_in.extend_from_slice(&m_bytes);
    md_in.extend_from_slice(&rb_bytes);
    md_in.extend_from_slice(&mprime_length);
    md_in.extend_from_slice(iv);

    // Compute SHA256 digest of md_in.
    let md = Sha256::digest(&md_in); // 32 bytes

    // Construct p = Y || M || Rb || md || (m_prime||length) || 8 bytes of 0x08.
    let mut p = Vec::new();
    p.extend_from_slice(&y_bytes);
    p.extend_from_slice(&m_bytes);
    p.extend_from_slice(&rb_bytes);
    p.extend_from_slice(&md);
    p.extend_from_slice(&mprime_length);
    p.extend_from_slice(&[0x08u8; 8]);

    // Verify that p is the expected length:
    // expected_len = (max_key_size * 3) + 32 + 8 + 8.
    let expected_len = (max_key_size * 3) + 32 + 8 + 8;
    assert_eq!(
        p.len(),
        expected_len,
        "P length mismatch: got {}, expected {}",
        p.len(),
        expected_len
    );

    // Allocate an output buffer exactly the same size as p.
    let mut out_buf = vec![0u8; p.len()];

    // Encrypt p using AES-256 in CBC mode with no padding.
    type Aes256CbcEnc = Encryptor<Aes256>;
    let ct = Aes256CbcEnc::new(aes_key.into(), iv.into())
        .encrypt_padded_b2b_mut::<NoPadding>(&p, &mut out_buf)
        .map_err(|e| format!("Encryption error: {:?}", e))?;

    let iv_and_ct = [iv, ct].concat();
    Ok(iv_and_ct)
}

/// Converts a fixed-length u32 array (representing a big number in little-endian order)
/// into a byte vector of exactly `max_bytes` length. Each u32 is converted using `to_le_bytes()`,
/// then the vector is truncated or padded with zeros to exactly max_bytes.
fn number_as_bytes(arr: &[u32; DS_MAX_WORDS], max_bytes: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DS_MAX_WORDS * 4);
    for &word in arr.iter() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    if bytes.len() > max_bytes {
        bytes.truncate(max_bytes);
    } else {
        while bytes.len() < max_bytes {
            bytes.push(0);
        }
    }
    bytes
}
