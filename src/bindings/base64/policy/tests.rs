use super::*;
use crate::base64::Base64Error;

fn prepared_policy() -> PreparedPolicy {
    PreparedPolicy {
        altchars: None,
        warning_altchars: None,
        warning_scan: WarningScan::Pending,
        urlsafe_warning: false,
        alphabet: None,
        validation: Validation::Lenient,
        padding: Padding::Padded,
        ignorechars_specified: false,
        ignored: None,
        canonical: false,
    }
}

#[test]
fn select_decode_routes() {
    let old = PythonSemantics::from_version((3, 14, 4));
    let new = PythonSemantics::from_version((3, 15, 0));

    for (policy, empty_ignorechars, semantics, expected) in [
        (
            PreparedPolicy {
                validation: Validation::Strict,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::Strict { urlsafe_315: false },
        ),
        (
            PreparedPolicy {
                altchars: Some(*b"-_"),
                warning_altchars: Some(*b"-_"),
                validation: Validation::Strict,
                padding: Padding::Unpadded,
                ..prepared_policy()
            },
            false,
            new,
            DecodeRoute::Strict { urlsafe_315: true },
        ),
        (
            prepared_policy(),
            false,
            old,
            DecodeRoute::LenientDirect { urlsafe_315: false },
        ),
        (
            PreparedPolicy {
                altchars: Some(*b"@#"),
                warning_altchars: Some(*b"@#"),
                padding: Padding::Unpadded,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::LenientCustom,
        ),
        (
            PreparedPolicy {
                canonical: true,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::Configured(ConfiguredShortcut::StandardStrict),
        ),
        (
            PreparedPolicy {
                padding: Padding::Unpadded,
                canonical: true,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::Configured(ConfiguredShortcut::CanonicalUnpadded),
        ),
        (
            PreparedPolicy {
                altchars: Some(*b"@#"),
                warning_altchars: Some(*b"@#"),
                validation: Validation::Strict,
                ignorechars_specified: true,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::Configured(ConfiguredShortcut::None),
        ),
        (
            PreparedPolicy {
                ignorechars_specified: true,
                ..prepared_policy()
            },
            false,
            old,
            DecodeRoute::Configured(ConfiguredShortcut::None),
        ),
        (
            PreparedPolicy {
                ignorechars_specified: true,
                ..prepared_policy()
            },
            true,
            old,
            DecodeRoute::Configured(ConfiguredShortcut::StandardStrict),
        ),
    ] {
        assert_eq!(
            select_route(&policy, empty_ignorechars, semantics),
            expected,
            "altchars={:?} validation={:?} padding={:?} ignorechars={} empty_ignorechars={empty_ignorechars} canonical={} semantics={semantics:?}",
            policy.altchars,
            policy.validation,
            policy.padding,
            policy.ignorechars_specified,
            policy.canonical,
        );
    }
}

#[test]
fn handle_probe_errors() {
    let small = Base64Error::OutputTooSmall {
        required: 3,
        provided: 2,
    };
    assert_eq!(
        DecodeAttempt::Probe.error_writes(),
        ErrorWrites::ValidatedPrefix
    );
    assert_eq!(DecodeAttempt::Strict.error_writes(), ErrorWrites::MayWrite);
    assert_eq!(DecodeAttempt::Probe.accept::<usize>(Err(small)), Ok(None));
    assert_eq!(
        DecodeAttempt::Strict.accept::<usize>(Err(small)),
        Err(small)
    );

    for attempt in [DecodeAttempt::Probe, DecodeAttempt::Strict] {
        assert_eq!(
            attempt.accept::<usize>(Err(Base64Error::InvalidInput)),
            Ok(None)
        );
        assert_eq!(attempt.accept(Ok(3)), Ok(Some(3)));
    }
}
