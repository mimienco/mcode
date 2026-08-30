use mcode_updater::{ReleaseChannel, ReleaseFeedConfiguration, ReleaseFeedConfigurationError};

fn endpoints() -> [(ReleaseChannel, String); 5] {
    [
        (
            ReleaseChannel::Stable,
            "https://updates.example.test/stable/{{target}}/{{arch}}/{{current_version}}"
                .to_owned(),
        ),
        (
            ReleaseChannel::Rc,
            "https://updates.example.test/rc/latest.json".to_owned(),
        ),
        (
            ReleaseChannel::Hourly,
            "https://updates.example.test/hourly/latest.json".to_owned(),
        ),
        (
            ReleaseChannel::Daily,
            "https://updates.example.test/daily/latest.json".to_owned(),
        ),
        (
            ReleaseChannel::Adhoc,
            "https://updates.example.test/adhoc/latest.json".to_owned(),
        ),
    ]
}

#[test]
fn every_release_channel_has_an_explicit_https_feed() {
    let config = ReleaseFeedConfiguration::new("public-key", endpoints()).unwrap();

    for channel in ReleaseChannel::ALL {
        assert!(config.endpoint(channel).as_str().starts_with("https://"));
    }
    assert_eq!(config.public_key(), "public-key");
}

#[test]
fn missing_channels_are_rejected_instead_of_falling_back_to_another_lane() {
    let error = ReleaseFeedConfiguration::new(
        "public-key",
        endpoints()
            .into_iter()
            .filter(|(channel, _)| *channel != ReleaseChannel::Adhoc),
    )
    .unwrap_err();

    assert_eq!(
        error,
        ReleaseFeedConfigurationError::MissingChannel(ReleaseChannel::Adhoc)
    );
}

#[test]
fn duplicate_and_insecure_feeds_are_rejected() {
    let duplicate = ReleaseFeedConfiguration::new(
        "public-key",
        endpoints().into_iter().chain([(
            ReleaseChannel::Stable,
            "https://other.test/latest.json".to_owned(),
        )]),
    )
    .unwrap_err();
    assert_eq!(
        duplicate,
        ReleaseFeedConfigurationError::DuplicateChannel(ReleaseChannel::Stable)
    );

    let duplicate_endpoint = ReleaseFeedConfiguration::new(
        "public-key",
        endpoints().into_iter().map(|(channel, endpoint)| {
            if channel == ReleaseChannel::Rc {
                (
                    channel,
                    "https://updates.example.test/stable/{{target}}/{{arch}}/{{current_version}}"
                        .to_owned(),
                )
            } else {
                (channel, endpoint)
            }
        }),
    )
    .unwrap_err();
    assert_eq!(
        duplicate_endpoint,
        ReleaseFeedConfigurationError::DuplicateEndpoint(ReleaseChannel::Rc)
    );

    let insecure = ReleaseFeedConfiguration::new(
        "public-key",
        endpoints().into_iter().map(|(channel, endpoint)| {
            if channel == ReleaseChannel::Hourly {
                (
                    channel,
                    "http://updates.example.test/hourly/latest.json".to_owned(),
                )
            } else {
                (channel, endpoint)
            }
        }),
    )
    .unwrap_err();
    assert_eq!(
        insecure,
        ReleaseFeedConfigurationError::InsecureEndpoint(ReleaseChannel::Hourly)
    );
}

#[test]
fn a_public_key_is_mandatory_and_is_never_rendered_in_errors() {
    let error = ReleaseFeedConfiguration::new("  ", endpoints()).unwrap_err();

    assert_eq!(error, ReleaseFeedConfigurationError::MissingPublicKey);
    assert!(!error.to_string().contains("public-key"));
}
