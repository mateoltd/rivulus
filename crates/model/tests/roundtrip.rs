//! P1 acceptance: >=50 fixtures round-trip + Unknown round-trips +
//! no-unknown-field-fail on docs-sample shapes + Event dispatch envelope mirror.
//!
//! Fixture files live in `tests/fixtures/` (workspace root). Tests run with
//! `CWD = crates/model`, so fixtures resolve via `CARGO_MANIFEST_DIR/../../tests/fixtures`.
use std::path::PathBuf;

fn fx(name: &str) -> Vec<u8> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("tests/fixtures");
    p.push(name);
    std::fs::read(p).unwrap()
}

/// Parse fixture `name` as `T`, spot-check it, then serde round-trip
/// (`T -> Value -> T`) and require the second parse to succeed.
fn rt<T>(name: &str, check: fn(&T))
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let raw = fx(name);
    let v: T = serde_json::from_slice(&raw).unwrap();
    check(&v);
    let val = serde_json::to_value(&v).unwrap();
    let back: T = serde_json::from_value(val).unwrap();
    let bytes = serde_json::to_vec(&back).unwrap();
    assert!(!bytes.is_empty());
}

#[derive(serde::Deserialize)]
struct PermWrap {
    p: model::Permissions,
}

// ---------- guild ----------

#[test]
fn rt_guild_json() {
    rt::<model::Guild>("guild.json", |g| assert_eq!(g.name.as_ref(), "T"));
}

#[test]
fn rt_guild_full_json() {
    rt::<model::Guild>("guild_full.json", |g| {
        assert_eq!(g.name.as_ref(), "Rivulus Test Guild");
        assert_eq!(g.member_count, Some(42));
        assert_eq!(g.roles.len(), 2);
        assert_eq!(g.emojis.len(), 2);
        assert_eq!(g.stickers.len(), 1);
        assert!(!g.unavailable);
    });
}

#[test]
fn rt_guild_unavailable_json() {
    rt::<model::UnavailableGuild>("guild_unavailable.json", |g| {
        assert_eq!(g.id.get(), 111_111_111_111_111_111);
        assert!(g.unavailable);
    });
}

// ---------- channels ----------

#[test]
fn rt_channel_json() {
    rt::<model::Channel>("channel.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildText));
        assert_eq!(c.name.as_deref(), Some("general"));
    });
}

#[test]
fn rt_channel_voice_json() {
    rt::<model::Channel>("channel_voice.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildVoice));
        assert!(c.guild_id.is_some());
    });
}

#[test]
fn rt_channel_dm_json() {
    rt::<model::Channel>("channel_dm.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::Dm));
    });
}

#[test]
fn rt_channel_group_json() {
    rt::<model::Channel>("channel_group.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GroupDm));
    });
}

#[test]
fn rt_channel_category_json() {
    rt::<model::Channel>("channel_category.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildCategory));
    });
}

#[test]
fn rt_channel_announcement_json() {
    rt::<model::Channel>("channel_announcement.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildAnnouncement));
        assert!(c.topic.is_some());
    });
}

#[test]
fn rt_channel_thread_announce_json() {
    rt::<model::Channel>("channel_thread_announce.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::AnnouncementThread));
        assert!(c.parent_id.is_some());
    });
}

#[test]
fn rt_channel_thread_public_json() {
    rt::<model::Channel>("channel_thread_public.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::PublicThread));
        assert!(c.parent_id.is_some());
    });
}

#[test]
fn rt_channel_thread_private_json() {
    rt::<model::Channel>("channel_thread_private.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::PrivateThread));
    });
}

#[test]
fn rt_channel_stage_json() {
    rt::<model::Channel>("channel_stage.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildStageVoice));
    });
}

#[test]
fn rt_channel_directory_json() {
    rt::<model::Channel>("channel_directory.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildDirectory));
    });
}

#[test]
fn rt_channel_forum_json() {
    rt::<model::Channel>("channel_forum.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildForum));
    });
}

#[test]
fn rt_channel_media_json() {
    rt::<model::Channel>("channel_media.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::GuildMedia));
    });
}

#[test]
fn rt_channel_unknown_json() {
    rt::<model::Channel>("channel_unknown.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::Unknown(99)));
    });
}

#[test]
fn rt_channel_unknown_200_json() {
    rt::<model::Channel>("channel_unknown_200.json", |c| {
        assert!(matches!(c.kind, model::ChannelType::Unknown(200)));
        assert_eq!(c.name.as_deref(), Some("future-kind"));
    });
}

#[test]
fn rt_channel_overwrites_json() {
    rt::<model::Channel>("channel_overwrites.json", |c| {
        assert_eq!(c.permission_overwrites.len(), 2);
        assert!(c.permission_overwrites[0]
            .allow
            .contains(model::Permissions::SEND_MESSAGES));
    });
}

#[test]
fn rt_overwrite_json() {
    rt::<model::Overwrite>("overwrite.json", |o| {
        assert_eq!(o.kind, 0);
        assert!(o.allow.contains(model::Permissions::SEND_MESSAGES));
    });
}

// ---------- messages ----------

#[test]
fn rt_message_json() {
    rt::<model::Message>("message.json", |m| {
        assert_eq!(m.author_id.get(), 2);
        assert!(m.author.is_some());
    });
}

#[test]
fn rt_message_embed_json() {
    rt::<model::Message>("message_embed.json", |m| {
        assert_eq!(m.embeds.len(), 2);
        assert_eq!(m.embeds[0].title.as_deref(), Some("Build #42"));
    });
}

#[test]
fn rt_message_attachments_json() {
    rt::<model::Message>("message_attachments.json", |m| {
        assert_eq!(m.attachments.len(), 2);
        assert_eq!(m.attachments[0].filename.as_ref(), "gateway.log");
        assert!(m.attachments[0].description.is_some());
    });
}

#[test]
fn rt_message_full_json() {
    rt::<model::Message>("message_full.json", |m| {
        assert_eq!(m.attachments.len(), 1);
        assert_eq!(m.embeds.len(), 1);
        assert!(m.edited_timestamp.is_some());
        assert!(m.author.as_ref().is_some_and(|u| u.bot));
    });
}

#[test]
fn rt_message_extra_keys_json() {
    // Docs-sample keys unknown to the model must not fail the parse.
    rt::<model::Message>("message_extra_keys.json", |m| {
        assert_eq!(m.content.as_ref(), "tolerant parse");
    });
}

// ---------- members ----------

#[test]
fn rt_member_json() {
    rt::<model::Member>("member.json", |m| {
        assert_eq!(m.user_id.get(), 2);
        assert_eq!(m.roles.len(), 1);
    });
}

#[test]
fn rt_member_full_json() {
    rt::<model::Member>("member_full.json", |m| {
        assert_eq!(m.nick.as_deref(), Some("riv"));
        assert_eq!(m.roles.len(), 2);
        assert!(m.joined_at.is_some());
        assert!(m.communication_disabled_until.is_some());
        assert!(!m.pending);
    });
}

#[test]
fn rt_member_pending_json() {
    rt::<model::Member>("member_pending.json", |m| {
        assert!(m.pending);
        assert!(m.roles.is_empty());
    });
}

// ---------- roles / permissions ----------

#[test]
fn rt_role_json() {
    rt::<model::Role>("role.json", |r| {
        assert_eq!(r.name.as_ref(), "mod");
    });
}

#[test]
fn rt_role_int_json() {
    rt::<model::Role>("role_int.json", |r| {
        assert!(r.permissions.contains(model::Permissions::ADMINISTRATOR));
    });
}

#[test]
fn rt_role_full_json() {
    rt::<model::Role>("role_full.json", |r| {
        assert!(r.hoist);
        assert!(r.mentionable);
        assert_eq!(r.color, 3_447_003);
        assert!(r.permissions.contains(model::Permissions::MANAGE_ROLES));
    });
}

#[test]
fn rt_permissions_str_json() {
    let w: PermWrap = serde_json::from_slice(&fx("permissions_str.json")).unwrap();
    assert!(
        w.p.contains(model::Permissions::ADD_REACTIONS | model::Permissions::SEND_MESSAGES)
            || w.p.contains(model::Permissions::SEND_MESSAGES)
    );
    let back = serde_json::to_value(&w.p).unwrap();
    let again: model::Permissions = serde_json::from_value(back).unwrap();
    assert_eq!(again, w.p);
}

#[test]
fn rt_permissions_int_json() {
    let w: PermWrap = serde_json::from_slice(&fx("permissions_int.json")).unwrap();
    assert!(w.p.contains(model::Permissions::SEND_MESSAGES));
    let back = serde_json::to_value(&w.p).unwrap();
    let again: model::Permissions = serde_json::from_value(back).unwrap();
    assert_eq!(again, w.p);
}

// ---------- emoji / sticker ----------

#[test]
fn rt_emoji_json() {
    rt::<model::Emoji>("emoji.json", |e| {
        assert_eq!(e.name.as_deref(), Some("wave"));
        assert!(!e.animated);
    });
}

#[test]
fn rt_emoji_animated_json() {
    rt::<model::Emoji>("emoji_animated.json", |e| assert!(e.animated));
}

#[test]
fn rt_emoji_unicode_json() {
    rt::<model::Emoji>("emoji_unicode.json", |e| {
        assert!(e.id.is_none());
        assert!(e.name.is_some());
    });
}

#[test]
fn rt_sticker_json() {
    rt::<model::Sticker>("sticker.json", |s| {
        assert_eq!(s.name.as_ref(), "hype");
        assert_eq!(s.format_type, 1);
    });
}

// ---------- invite / webhook ----------

#[test]
fn rt_invite_json() {
    rt::<model::Invite>("invite.json", |i| {
        assert_eq!(i.code.as_ref(), "abcXYZ12");
        assert!(i.channel_id.is_some());
        assert!(i.guild_id.is_some());
    });
}

#[test]
fn rt_webhook_json() {
    rt::<model::Webhook>("webhook.json", |w| {
        assert_eq!(w.name.as_deref(), Some("deploy-hook"));
        assert!(w.token.is_some());
    });
}

// ---------- threads ----------

#[test]
fn rt_thread_json() {
    rt::<model::Thread>("thread.json", |t| {
        assert_eq!(t.name.as_deref(), Some("qotd-thread"));
        assert!(t.parent_id.is_some());
    });
}

#[test]
fn rt_thread_member_json() {
    rt::<model::ThreadMember>("thread_member.json", |m| {
        assert!(m.user_id.is_some());
        assert!(m.id.is_some());
    });
}

#[test]
fn rt_thread_metadata_json() {
    rt::<model::ThreadMetadata>("thread_metadata.json", |m| {
        assert!(!m.archived);
        assert_eq!(m.auto_archive_duration, 1440);
        assert!(!m.locked);
    });
}

// ---------- polls ----------

#[test]
fn rt_poll_json() {
    rt::<model::Poll>("poll.json", |p| {
        assert_eq!(p.question.as_deref(), Some("Ship Friday?"));
        assert_eq!(p.answers.len(), 3);
    });
}

#[test]
fn rt_poll_answer_json() {
    rt::<model::PollAnswer>("poll_answer.json", |a| {
        assert_eq!(a.answer_id, 1);
        assert_eq!(a.text.as_deref(), Some("Yes"));
    });
}

// ---------- scheduled / stage / soundboard ----------

#[test]
fn rt_scheduled_json() {
    rt::<model::ScheduledEvent>("scheduled.json", |e| {
        assert_eq!(e.name.as_deref(), Some("Release party"));
        assert!(e.guild_id.is_some());
    });
}

#[test]
fn rt_stage_json() {
    rt::<model::StageInstance>("stage.json", |s| {
        assert_eq!(s.topic.as_deref(), Some("Weekly sync"));
    });
}

#[test]
fn rt_soundboard_json() {
    rt::<model::SoundboardSound>("soundboard.json", |s| {
        assert_eq!(s.name.as_deref(), Some("airhorn"));
        assert_eq!(s.sound_id.get(), 666_666_666_666_666_666);
    });
}

// ---------- entitlement / sku ----------

#[test]
fn rt_entitlement_json() {
    rt::<model::Entitlement>("entitlement.json", |e| {
        assert!(e.user_id.is_some());
        assert!(e.guild_id.is_some());
    });
}

#[test]
fn rt_sku_json() {
    rt::<model::Sku>("sku.json", |s| {
        assert_eq!(s.name.as_deref(), Some("Pro Tier"));
    });
}

// ---------- automod ----------

#[test]
fn rt_automod_rule_json() {
    rt::<model::AutoModRule>("automod_rule.json", |r| {
        assert_eq!(r.name.as_deref(), Some("no-spam-links"));
    });
}

#[test]
fn rt_automod_action_json() {
    rt::<model::AutoModAction>("automod_action.json", |a| {
        assert!(a.guild_id.is_some());
        assert!(a.rule_id.is_some());
    });
}

// ---------- audit / onboarding ----------

#[test]
fn rt_audit_json() {
    rt::<model::AuditEntry>("audit.json", |a| {
        assert_eq!(a.id.get(), 10);
        assert_eq!(a.action_type, 1);
    });
}

#[test]
fn rt_audit_full_json() {
    rt::<model::AuditEntry>("audit_full.json", |a| {
        assert_eq!(a.action_type, 24);
        assert_eq!(a.reason.as_deref(), Some("spam cleanup"));
        assert!(a.user_id.is_some());
    });
}

#[test]
fn rt_onboarding_json() {
    rt::<model::OnboardingPrompt>("onboarding.json", |o| {
        assert_eq!(o.title.as_deref(), Some("Welcome aboard"));
    });
}

// ---------- interactions ----------

#[test]
fn rt_interaction_json() {
    rt::<model::Interaction>("interaction.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::ApplicationCommand));
    });
}

#[test]
fn rt_interaction_ping_json() {
    rt::<model::Interaction>("interaction_ping.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::Ping));
    });
}

#[test]
fn rt_interaction_command_json() {
    rt::<model::Interaction>("interaction_command.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::ApplicationCommand));
        assert!(i.guild_id.is_some());
    });
}

#[test]
fn rt_interaction_component_json() {
    rt::<model::Interaction>("interaction_component.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::MessageComponent));
        assert_eq!(i.custom_id.as_deref(), Some("btn:approve"));
    });
}

#[test]
fn rt_interaction_autocomplete_json() {
    rt::<model::Interaction>("interaction_autocomplete.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::Autocomplete));
    });
}

#[test]
fn rt_interaction_modal_json() {
    rt::<model::Interaction>("interaction_modal.json", |i| {
        assert!(matches!(i.kind, model::InteractionType::ModalSubmit));
        assert_eq!(i.custom_id.as_deref(), Some("modal:feedback"));
    });
}

#[test]
fn rt_commandoption_str_json() {
    rt::<model::CommandOption>("commandoption_str.json", |o| {
        assert_eq!(o.name.as_ref(), "repo");
        assert!(matches!(o.value, Some(model::OptionValue::Str(_))));
    });
}

#[test]
fn rt_commandoption_int_json() {
    rt::<model::CommandOption>("commandoption_int.json", |o| {
        assert!(matches!(o.value, Some(model::OptionValue::Int(25))));
    });
}

#[test]
fn rt_commandoption_bool_json() {
    rt::<model::CommandOption>("commandoption_bool.json", |o| {
        assert!(matches!(o.value, Some(model::OptionValue::Bool(true))));
    });
}

// ---------- components ----------

#[test]
fn rt_components_v2_json() {
    rt::<model::Component>("components_v2.json", |c| {
        assert!(matches!(c.kind, model::ComponentKind::TextDisplay));
        assert_eq!(c.content.as_deref(), Some("hello world"));
    });
}

#[test]
fn rt_component_button_json() {
    rt::<model::Component>("component_button.json", |c| {
        assert!(matches!(c.kind, model::ComponentKind::Button));
        assert_eq!(c.label.as_deref(), Some("Approve"));
        assert!(matches!(c.style, Some(model::ButtonStyle::Primary)));
    });
}

#[test]
fn rt_component_action_row_json() {
    rt::<model::Component>("component_action_row.json", |c| {
        assert!(matches!(c.kind, model::ComponentKind::ActionRow));
        assert_eq!(c.components.len(), 2);
        model::components_v2::validate_v1(std::slice::from_ref(c)).unwrap();
    });
}

#[test]
fn rt_component_container_json() {
    rt::<model::Component>("component_container.json", |c| {
        assert!(matches!(c.kind, model::ComponentKind::Container));
        assert_eq!(c.components.len(), 1);
        model::components_v2::validate_v2(std::slice::from_ref(c)).unwrap();
    });
}

#[test]
fn rt_component_section_json() {
    rt::<model::Component>("component_section.json", |c| {
        assert!(matches!(c.kind, model::ComponentKind::Section));
        assert_eq!(c.components.len(), 1);
    });
}

// ---------- presence / voice ----------

#[test]
fn rt_presence_json() {
    rt::<model::Presence>("presence.json", |p| {
        assert!(matches!(p.status, Some(model::Status::Online)));
        assert_eq!(p.activities.len(), 1);
    });
}

#[test]
fn rt_activity_json() {
    rt::<model::Activity>("activity.json", |a| {
        assert_eq!(a.name.as_ref(), "Competing in hackathon");
        assert_eq!(a.kind, 5);
    });
}

#[test]
fn rt_voice_state_json() {
    rt::<model::VoiceState>("voice_state.json", |v| {
        assert_eq!(v.session_id.as_deref(), Some("sess_abc123"));
        assert!(v.deaf);
        assert!(!v.mute);
    });
}

#[test]
fn rt_voice_server_json() {
    rt::<model::VoiceServerUpdate>("voice_server.json", |v| {
        assert_eq!(v.token.as_ref(), "vs_tok_abc");
        assert!(v.endpoint.is_some());
    });
}

// ---------- gateway / user / chunk ----------

#[test]
fn rt_gateway_bot_json() {
    rt::<model::GetGatewayBotResponse>("gateway_bot.json", |g| {
        assert_eq!(g.shards, 9);
        assert_eq!(g.session_start_limit.remaining, 999);
    });
}

#[test]
fn rt_ready_json() {
    rt::<model::Ready>("ready.json", |r| {
        assert_eq!(r.v, 10);
        assert_eq!(r.session_id.as_ref(), "sess_ready_123");
        assert!(r.user.bot);
    });
}

#[test]
fn rt_user_json() {
    rt::<model::User>("user.json", |u| {
        assert_eq!(u.username.as_ref(), "rivulet");
        assert_eq!(u.global_name.as_deref(), Some("Rivulet"));
    });
}

#[test]
fn rt_chunk_json() {
    rt::<model::GuildMembersChunk>("chunk.json", |c| {
        assert_eq!(c.chunk_index, 0);
        assert_eq!(c.chunk_count, 1);
    });
}

#[test]
fn rt_chunk2_json() {
    rt::<model::GuildMembersChunk>("chunk2.json", |c| {
        assert_eq!(c.chunk_index, 1);
        assert_eq!(c.chunk_count, 2);
    });
}

#[test]
fn rt_chunk_full_json() {
    rt::<model::GuildMembersChunk>("chunk_full.json", |c| {
        assert_eq!(c.nonce.as_deref(), Some("members-1"));
        assert_eq!(c.members.len(), 2);
        assert_eq!(c.not_found.len(), 2);
        assert_eq!(c.members[0].nick.as_deref(), Some("riv"));
    });
}

// ---------- Unknown(u8) round-trips ----------

#[test]
fn unknown_channeltype_known_values() {
    let cases = [
        (0u8, model::ChannelType::GuildText),
        (1, model::ChannelType::Dm),
        (2, model::ChannelType::GuildVoice),
        (3, model::ChannelType::GroupDm),
        (4, model::ChannelType::GuildCategory),
        (5, model::ChannelType::GuildAnnouncement),
        (10, model::ChannelType::AnnouncementThread),
        (11, model::ChannelType::PublicThread),
        (12, model::ChannelType::PrivateThread),
        (13, model::ChannelType::GuildStageVoice),
        (14, model::ChannelType::GuildDirectory),
        (15, model::ChannelType::GuildForum),
        (16, model::ChannelType::GuildMedia),
    ];
    for (n, want) in cases {
        let got: model::ChannelType = serde_json::from_value(serde_json::Value::from(n)).unwrap();
        assert_eq!(got, want);
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            serde_json::Value::from(n)
        );
    }
}

#[test]
fn unknown_channeltype_roundtrips() {
    for n in [17u8, 42, 99, 200, 255] {
        let v = serde_json::Value::from(n);
        let got: model::ChannelType = serde_json::from_value(v).unwrap();
        assert!(matches!(got, model::ChannelType::Unknown(m) if m == n));
        let back = serde_json::to_value(got).unwrap();
        assert_eq!(back, serde_json::Value::from(n));
        let again: model::ChannelType = serde_json::from_value(back).unwrap();
        assert!(matches!(again, model::ChannelType::Unknown(m) if m == n));
    }
}

#[test]
fn unknown_scope_roundtrips() {
    // Unknown scopes keep their raw string (NOT a lossy `#[serde(other)]`).
    let got: model::Scope =
        serde_json::from_value(serde_json::Value::from("definitely-not-a-scope")).unwrap();
    assert!(matches!(got, model::Scope::Unknown(_)));
    let back = serde_json::to_value(&got).unwrap();
    assert_eq!(back, serde_json::Value::from("definitely-not-a-scope"));
    let again: model::Scope = serde_json::from_value(back).unwrap();
    assert_eq!(again, got);
}

#[test]
fn scope_known_wire_strings() {
    // Discord sends lowercase scope strings, not variant names.
    let cases = [
        ("bot", model::Scope::Bot),
        ("applications.commands", model::Scope::ApplicationsCommands),
        ("identify", model::Scope::Identify),
        ("guilds", model::Scope::Guilds),
    ];
    for (s, want) in cases {
        let got: model::Scope = serde_json::from_value(serde_json::Value::from(s)).unwrap();
        assert_eq!(got, want);
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            serde_json::Value::from(s)
        );
    }
}

#[test]
fn unknown_event_cap() {
    let big = "x".repeat(20000);
    let e = model::Event::unknown("FOO", Some(1), &big);
    if let model::Event::Unknown { payload, .. } = e {
        assert!(payload.len() <= model::events::UNKNOWN_CAP);
    } else {
        panic!("wrong variant");
    }
}

// ---------- misc kept ----------

#[test]
fn intents_aliases() {
    assert_eq!(
        model::Intents::GUILD_BANS.bits(),
        model::Intents::GUILD_MODERATION.bits()
    );
    assert_eq!(
        model::Intents::GUILD_EXPRESSIONS.bits(),
        model::Intents::GUILD_EMOJIS_AND_STICKERS.bits()
    );
}

#[test]
fn snowflake_display_parse() {
    let id: model::GuildId = "123".parse().unwrap();
    assert_eq!(id.to_string(), "123");
    assert_eq!(id.get(), 123);
}

// ---------- no unknown-field fail ----------

#[test]
fn docs_samples_ignore_unknown_fields() {
    // Simulates docs-sample drift: extra keys must not fail the parse.
    let raw = br#"{"id":"111111111111111111","name":"T","owner_id":"2","future_object":{"a":[1,2]},"future_flag":true}"#;
    let g: model::Guild = serde_json::from_slice(raw).unwrap();
    assert_eq!(g.name.as_ref(), "T");
    let raw = br#"{"id":"4","channel_id":"3","author_id":"2","content":"hi","timestamp":"2024-01-01T00:00:00Z","nonce":"n","referenced_message":null}"#;
    let m: model::Message = serde_json::from_slice(raw).unwrap();
    assert_eq!(m.content.as_ref(), "hi");
}

// ---------- Event dispatch envelope mirror ----------

/// Build a gateway dispatch envelope (`op 0`) around a fixture payload.
fn envelope(t: &str, s: u64, fixture: &str) -> Vec<u8> {
    let d = String::from_utf8(fx(fixture)).unwrap();
    format!(r#"{{"op":0,"t":"{t}","s":{s},"d":{d}}}"#).into_bytes()
}

fn header_check(raw: &[u8], t: &str, s: u64) -> serde_json::Value {
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.op, 0);
    assert_eq!(h.t, Some(t));
    assert_eq!(h.s, Some(s));
    common::json::from_raw(h.d).unwrap()
}

#[test]
fn env_ready() {
    let raw = envelope("READY", 1, "ready.json");
    let v = header_check(&raw, "READY", 1);
    let r: model::Ready = serde_json::from_value(v).unwrap();
    let e = model::Event::Ready(std::sync::Arc::new(r));
    assert_eq!(e.kind(), "READY");
}

#[test]
fn env_guild_create() {
    let raw = envelope("GUILD_CREATE", 2, "guild_full.json");
    let v = header_check(&raw, "GUILD_CREATE", 2);
    let g: model::Guild = serde_json::from_value(v).unwrap();
    let e = model::Event::GuildCreate(std::sync::Arc::new(g));
    assert!(matches!(e, model::Event::GuildCreate(_)));
}

#[test]
fn env_message_create() {
    let raw = envelope("MESSAGE_CREATE", 7, "message_full.json");
    let v = header_check(&raw, "MESSAGE_CREATE", 7);
    let m: model::Message = serde_json::from_value(v).unwrap();
    assert_eq!(m.content.as_ref(), "Hello <@333333333333333333> :wave:");
    let e = model::Event::MessageCreate(std::sync::Arc::new(m));
    assert_eq!(e.kind(), "MESSAGE_CREATE");
}

#[test]
fn env_channel_create() {
    let raw = envelope("CHANNEL_CREATE", 8, "channel_voice.json");
    let v = header_check(&raw, "CHANNEL_CREATE", 8);
    let c: model::Channel = serde_json::from_value(v).unwrap();
    let e = model::Event::ChannelCreate(std::sync::Arc::new(c));
    assert!(matches!(e, model::Event::ChannelCreate(_)));
}

#[test]
fn env_member_add() {
    let raw = envelope("GUILD_MEMBER_ADD", 9, "member_full.json");
    let v = header_check(&raw, "GUILD_MEMBER_ADD", 9);
    let m: model::Member = serde_json::from_value(v).unwrap();
    let e = model::Event::MemberAdd(std::sync::Arc::new(m));
    assert!(matches!(e, model::Event::MemberAdd(_)));
}

#[test]
fn env_members_chunk() {
    let raw = envelope("GUILD_MEMBERS_CHUNK", 10, "chunk_full.json");
    let v = header_check(&raw, "GUILD_MEMBERS_CHUNK", 10);
    let c: model::GuildMembersChunk = serde_json::from_value(v).unwrap();
    assert_eq!(c.members.len(), 2);
    let e = model::Event::MembersChunk(std::sync::Arc::new(c));
    assert!(matches!(e, model::Event::MembersChunk(_)));
}

#[test]
fn env_interaction_create() {
    let raw = envelope("INTERACTION_CREATE", 11, "interaction_command.json");
    let v = header_check(&raw, "INTERACTION_CREATE", 11);
    let i: model::Interaction = serde_json::from_value(v).unwrap();
    let e = model::Event::InteractionCreate(std::sync::Arc::new(i));
    assert_eq!(e.kind(), "INTERACTION_CREATE");
}

#[test]
fn env_presence_update() {
    let raw = envelope("PRESENCE_UPDATE", 12, "presence.json");
    let v = header_check(&raw, "PRESENCE_UPDATE", 12);
    let p: model::Presence = serde_json::from_value(v).unwrap();
    let e = model::Event::PresenceUpdate(std::sync::Arc::new(p));
    assert!(matches!(e, model::Event::PresenceUpdate(_)));
}

#[test]
fn env_voice_state_update() {
    let raw = envelope("VOICE_STATE_UPDATE", 13, "voice_state.json");
    let v = header_check(&raw, "VOICE_STATE_UPDATE", 13);
    let s: model::VoiceState = serde_json::from_value(v).unwrap();
    let e = model::Event::VoiceStateUpdate(std::sync::Arc::new(s));
    assert!(matches!(e, model::Event::VoiceStateUpdate(_)));
}

#[test]
fn env_voice_server_update() {
    let raw = envelope("VOICE_SERVER_UPDATE", 14, "voice_server.json");
    let v = header_check(&raw, "VOICE_SERVER_UPDATE", 14);
    let s: model::VoiceServerUpdate = serde_json::from_value(v).unwrap();
    let e = model::Event::VoiceServerUpdate(std::sync::Arc::new(s));
    assert!(matches!(e, model::Event::VoiceServerUpdate(_)));
}

#[test]
fn env_invite_create() {
    let raw = envelope("INVITE_CREATE", 15, "invite.json");
    let v = header_check(&raw, "INVITE_CREATE", 15);
    let i: model::Invite = serde_json::from_value(v).unwrap();
    let e = model::Event::InviteCreate(std::sync::Arc::new(i));
    assert!(matches!(e, model::Event::InviteCreate(_)));
}

#[test]
fn env_scheduled_create() {
    let raw = envelope("GUILD_SCHEDULED_EVENT_CREATE", 16, "scheduled.json");
    let v = header_check(&raw, "GUILD_SCHEDULED_EVENT_CREATE", 16);
    let s: model::ScheduledEvent = serde_json::from_value(v).unwrap();
    let e = model::Event::ScheduledCreate(std::sync::Arc::new(s));
    assert!(matches!(e, model::Event::ScheduledCreate(_)));
}

#[test]
fn env_stage_create() {
    let raw = envelope("STAGE_INSTANCE_CREATE", 17, "stage.json");
    let v = header_check(&raw, "STAGE_INSTANCE_CREATE", 17);
    let s: model::StageInstance = serde_json::from_value(v).unwrap();
    let e = model::Event::StageCreate(std::sync::Arc::new(s));
    assert!(matches!(e, model::Event::StageCreate(_)));
}

#[test]
fn env_automod_rule_create() {
    let raw = envelope("AUTO_MODERATION_RULE_CREATE", 18, "automod_rule.json");
    let v = header_check(&raw, "AUTO_MODERATION_RULE_CREATE", 18);
    let r: model::AutoModRule = serde_json::from_value(v).unwrap();
    let e = model::Event::AutoModRuleCreate(std::sync::Arc::new(r));
    assert!(matches!(e, model::Event::AutoModRuleCreate(_)));
}

#[test]
fn env_automod_action() {
    let raw = envelope(
        "AUTO_MODERATION_ACTION_EXECUTION",
        19,
        "automod_action.json",
    );
    let v = header_check(&raw, "AUTO_MODERATION_ACTION_EXECUTION", 19);
    let a: model::AutoModAction = serde_json::from_value(v).unwrap();
    let e = model::Event::AutoModAction(std::sync::Arc::new(a));
    assert!(matches!(e, model::Event::AutoModAction(_)));
}

#[test]
fn env_audit_entry() {
    let raw = envelope("GUILD_AUDIT_LOG_ENTRY_CREATE", 20, "audit_full.json");
    let v = header_check(&raw, "GUILD_AUDIT_LOG_ENTRY_CREATE", 20);
    let a: model::AuditEntry = serde_json::from_value(v).unwrap();
    let e = model::Event::AuditEntry(std::sync::Arc::new(a));
    assert!(matches!(e, model::Event::AuditEntry(_)));
}

#[test]
fn env_entitlement_create() {
    let raw = envelope("ENTITLEMENT_CREATE", 21, "entitlement.json");
    let v = header_check(&raw, "ENTITLEMENT_CREATE", 21);
    let e_: model::Entitlement = serde_json::from_value(v).unwrap();
    let e = model::Event::EntitlementCreate(std::sync::Arc::new(e_));
    assert!(matches!(e, model::Event::EntitlementCreate(_)));
}

#[test]
fn env_thread_create() {
    let raw = envelope("THREAD_CREATE", 22, "thread.json");
    let v = header_check(&raw, "THREAD_CREATE", 22);
    let t: model::Thread = serde_json::from_value(v).unwrap();
    let e = model::Event::ThreadCreate(std::sync::Arc::new(t));
    assert!(matches!(e, model::Event::ThreadCreate(_)));
}

#[test]
fn env_thread_member_update() {
    let raw = envelope("THREAD_MEMBER_UPDATE", 23, "thread_member.json");
    let v = header_check(&raw, "THREAD_MEMBER_UPDATE", 23);
    let m: model::ThreadMember = serde_json::from_value(v).unwrap();
    let e = model::Event::ThreadMemberUpdate(std::sync::Arc::new(m));
    assert!(matches!(e, model::Event::ThreadMemberUpdate(_)));
}

#[test]
fn env_user_update() {
    let raw = envelope("USER_UPDATE", 24, "user.json");
    let v = header_check(&raw, "USER_UPDATE", 24);
    let u: model::User = serde_json::from_value(v).unwrap();
    let e = model::Event::UserUpdate(std::sync::Arc::new(u));
    assert!(matches!(e, model::Event::UserUpdate(_)));
}

#[test]
fn env_role_create() {
    let role = String::from_utf8(fx("role_full.json")).unwrap();
    let raw =
        format!(r#"{{"op":0,"t":"GUILD_ROLE_CREATE","s":25,"d":{{"guild_id":"111111111111111111","role":{role}}}}}"#)
            .into_bytes();
    let h: common::json::Header<'_> = serde_json::from_slice(&raw).unwrap();
    assert_eq!(h.op, 0);
    assert_eq!(h.t, Some("GUILD_ROLE_CREATE"));
    assert_eq!(h.s, Some(25));
    let d: serde_json::Value = common::json::from_raw(h.d).unwrap();
    let gid: model::GuildId = serde_json::from_value(d["guild_id"].clone()).unwrap();
    let role: model::Role = serde_json::from_value(d["role"].clone()).unwrap();
    let e = model::Event::RoleCreate {
        guild_id: gid,
        role: std::sync::Arc::new(role),
    };
    assert!(matches!(e, model::Event::RoleCreate { .. }));
}

#[test]
fn env_reaction_add() {
    let raw =
        br#"{"op":0,"t":"MESSAGE_REACTION_ADD","s":26,"d":{"channel_id":"222222222222222222","message_id":"444444444444444444","user_id":"333333333333333333","emoji":"wave"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("MESSAGE_REACTION_ADD"));
    let d: serde_json::Value = common::json::from_raw(h.d).unwrap();
    assert_eq!(d["emoji"], serde_json::Value::from("wave"));
    let e = model::Event::ReactionAdd {
        channel_id: "222222222222222222".parse().unwrap(),
        message_id: "444444444444444444".parse().unwrap(),
        user_id: "333333333333333333".parse().unwrap(),
        emoji: Box::from("wave"),
    };
    assert!(matches!(e, model::Event::ReactionAdd { .. }));
}

#[test]
fn env_typing_start() {
    let raw =
        br#"{"op":0,"t":"TYPING_START","s":27,"d":{"channel_id":"222222222222222222","user_id":"333333333333333333"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("TYPING_START"));
    let d: serde_json::Value = common::json::from_raw(h.d).unwrap();
    assert_eq!(
        d["channel_id"],
        serde_json::Value::from("222222222222222222")
    );
    let e = model::Event::TypingStart {
        channel_id: "222222222222222222".parse().unwrap(),
        user_id: "333333333333333333".parse().unwrap(),
    };
    assert!(matches!(e, model::Event::TypingStart { .. }));
}

#[test]
fn env_pins_update() {
    let raw =
        br#"{"op":0,"t":"CHANNEL_PINS_UPDATE","s":28,"d":{"channel_id":"222222222222222222","last_pin":"2024-05-01T12:34:56.000Z"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("CHANNEL_PINS_UPDATE"));
    let d: serde_json::Value = common::json::from_raw(h.d).unwrap();
    assert!(d["last_pin"].is_string());
    let e = model::Event::ChannelPinsUpdate {
        channel_id: "222222222222222222".parse().unwrap(),
        last_pin: Some(Box::from("2024-05-01T12:34:56.000Z")),
    };
    assert!(matches!(e, model::Event::ChannelPinsUpdate { .. }));
}

#[test]
fn env_message_delete() {
    let raw =
        br#"{"op":0,"t":"MESSAGE_DELETE","s":29,"d":{"channel_id":"222222222222222222","message_id":"444444444444444444"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("MESSAGE_DELETE"));
    let e = model::Event::MessageDelete {
        channel_id: "222222222222222222".parse().unwrap(),
        message_id: "444444444444444444".parse().unwrap(),
    };
    assert!(matches!(e, model::Event::MessageDelete { .. }));
}

#[test]
fn env_message_delete_bulk() {
    let raw =
        br#"{"op":0,"t":"MESSAGE_DELETE_BULK","s":30,"d":{"channel_id":"222222222222222222","ids":["444444444444444444"]}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("MESSAGE_DELETE_BULK"));
    let d: serde_json::Value = common::json::from_raw(h.d).unwrap();
    let ids: Vec<model::MessageId> = serde_json::from_value(d["ids"].clone()).unwrap();
    let e = model::Event::MessageDeleteBulk {
        channel_id: "222222222222222222".parse().unwrap(),
        ids,
    };
    if let model::Event::MessageDeleteBulk { ids, .. } = e {
        assert_eq!(ids.len(), 1);
    } else {
        panic!("wrong variant");
    }
}

#[test]
fn env_poll_vote_add() {
    let raw =
        br#"{"op":0,"t":"MESSAGE_POLL_VOTE_ADD","s":31,"d":{"message_id":"444444444444444444","answer_id":2}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("MESSAGE_POLL_VOTE_ADD"));
    let e = model::Event::PollVoteAdd {
        message_id: "444444444444444444".parse().unwrap(),
        answer_id: 2,
    };
    assert!(matches!(e, model::Event::PollVoteAdd { answer_id: 2, .. }));
}

#[test]
fn env_thread_list_sync() {
    let raw = br#"{"op":0,"t":"THREAD_LIST_SYNC","s":32,"d":{"guild_id":"111111111111111111"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("THREAD_LIST_SYNC"));
    let e = model::Event::ThreadListSync {
        guild_id: "111111111111111111".parse().unwrap(),
    };
    assert!(matches!(e, model::Event::ThreadListSync { .. }));
}

#[test]
fn env_invite_delete() {
    let raw =
        br#"{"op":0,"t":"INVITE_DELETE","s":33,"d":{"channel_id":"222222222222222222","code":"abcXYZ12"}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("INVITE_DELETE"));
    let e = model::Event::InviteDelete {
        channel_id: "222222222222222222".parse().unwrap(),
        code: Box::from("abcXYZ12"),
    };
    assert!(matches!(e, model::Event::InviteDelete { .. }));
}

#[test]
fn env_guild_delete() {
    let raw =
        br#"{"op":0,"t":"GUILD_DELETE","s":34,"d":{"guild_id":"111111111111111111","unavailable":false}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("GUILD_DELETE"));
    let e = model::Event::GuildDelete {
        guild_id: "111111111111111111".parse().unwrap(),
        unavailable: false,
    };
    assert!(matches!(e, model::Event::GuildDelete { .. }));
}

#[test]
fn env_unknown_dispatch() {
    let raw =
        br#"{"op":0,"t":"FUTURE_FAKE_EVENT","s":35,"d":{"future":true,"nested":{"a":[1,2,3]}}}"#;
    let h: common::json::Header<'_> = serde_json::from_slice(raw).unwrap();
    assert_eq!(h.t, Some("FUTURE_FAKE_EVENT"));
    let e = model::Event::unknown("FUTURE_FAKE_EVENT", h.s, h.d.get());
    assert_eq!(e.kind(), "UNKNOWN");
    if let model::Event::Unknown { kind, seq, payload } = e {
        assert_eq!(kind.as_ref(), "FUTURE_FAKE_EVENT");
        assert_eq!(seq, Some(35));
        assert!(payload.contains("future"));
    } else {
        panic!("wrong variant");
    }
}

// ---------- Discord int wire format (plans/04 §C+§D) ----------

/// Require int `T -> Value -> T` identity for every known number.
fn int_rt<T>(cases: &[(u8, T)])
where
    T: serde::de::DeserializeOwned + serde::Serialize + PartialEq + std::fmt::Debug + Copy,
{
    for (n, want) in cases {
        let got: T = serde_json::from_value(serde_json::Value::from(*n)).unwrap();
        assert_eq!(got, *want);
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            serde_json::Value::from(*n)
        );
    }
}

/// Require `Unknown` int round-trip for future Discord numbers.
fn unknown_rt<T>(ns: &[u8], is_unknown: fn(&T, u8) -> bool)
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    for n in ns {
        let v = serde_json::Value::from(*n);
        let got: T = serde_json::from_value(v).unwrap();
        assert!(is_unknown(&got, *n));
        let back = serde_json::to_value(&got).unwrap();
        assert_eq!(back, serde_json::Value::from(*n));
    }
}

#[test]
fn discord_interaction_type_ints() {
    use model::InteractionType as T;
    int_rt(&[
        (1, T::Ping),
        (2, T::ApplicationCommand),
        (3, T::MessageComponent),
        (4, T::Autocomplete),
        (5, T::ModalSubmit),
    ]);
    unknown_rt(
        &[0, 6, 200, 255],
        |g: &T, n| matches!(g, T::Unknown(m) if *m == n),
    );
}

#[test]
fn discord_callback_type_ints() {
    use model::CallbackType as T;
    int_rt(&[
        (1, T::Pong),
        (4, T::ChannelMessage),
        (5, T::DeferredChannel),
        (6, T::DeferredUpdate),
        (7, T::UpdateMessage),
        (8, T::AutocompleteResult),
        (9, T::Modal),
        (10, T::PremiumRequired),
        (12, T::LaunchActivity),
    ]);
    // No 11 on the wire; it must survive as Unknown, not error.
    unknown_rt(
        &[0, 2, 3, 11, 200],
        |g: &T, n| matches!(g, T::Unknown(m) if *m == n),
    );
}

#[test]
fn discord_component_kind_ints() {
    use model::ComponentKind as T;
    int_rt(&[
        (1, T::ActionRow),
        (2, T::Button),
        (3, T::StringSelect),
        (4, T::TextInput),
        (5, T::UserSelect),
        (6, T::RoleSelect),
        (7, T::MentionableSelect),
        (8, T::ChannelSelect),
        (9, T::Section),
        (10, T::TextDisplay),
        (11, T::Thumbnail),
        (12, T::MediaGallery),
        (13, T::File),
        (14, T::Separator),
        (17, T::Container),
        (18, T::Label),
    ]);
    // Gaps (0/15/16/19+) are future kinds: Unknown, not error.
    unknown_rt(
        &[0, 15, 16, 19, 200],
        |g: &T, n| matches!(g, T::Unknown(m) if *m == n),
    );
}

#[test]
fn discord_button_style_ints() {
    use model::ButtonStyle as T;
    int_rt(&[
        (1, T::Primary),
        (2, T::Secondary),
        (3, T::Success),
        (4, T::Danger),
        (5, T::Link),
        (6, T::Premium),
    ]);
    unknown_rt(
        &[0, 7, 200],
        |g: &T, n| matches!(g, T::Unknown(m) if *m == n),
    );
}

#[test]
fn discord_interaction_type_key() {
    // Real Discord payload: `"type": 3` int under the `type` key.
    let raw =
        br#"{"id":"161616161616161616","application_id":"888888888888888888","type":3,"token":"tok","custom_id":"btn:x"}"#;
    let i: model::Interaction = serde_json::from_slice(raw).unwrap();
    assert!(matches!(i.kind, model::InteractionType::MessageComponent));
    assert_eq!(i.custom_id.as_deref(), Some("btn:x"));
    // Serialized form keeps the `type` int key (Discord-shaped).
    let v = serde_json::to_value(&i).unwrap();
    assert_eq!(v["type"], serde_json::Value::from(3));
}

#[test]
fn discord_message_derives_author_id() {
    // Discord MESSAGE_CREATE: `author` object, no `author_id`.
    let raw =
        br#"{"id":"4","channel_id":"3","author":{"id":"2","username":"u","discriminator":"0"},"content":"hi","timestamp":"2024-01-01T00:00:00Z"}"#;
    let m: model::Message = serde_json::from_slice(raw).unwrap();
    assert_eq!(m.author_id.get(), 2);
    assert!(m.author.is_some());
    // Explicit `author_id` (model-shape) is still tolerated.
    let raw =
        br#"{"id":"4","channel_id":"3","author_id":"2","content":"hi","timestamp":"2024-01-01T00:00:00Z"}"#;
    let m: model::Message = serde_json::from_slice(raw).unwrap();
    assert_eq!(m.author_id.get(), 2);
    assert!(m.author.is_none());
    // Neither key => error (no silent zero id).
    let raw = br#"{"id":"4","channel_id":"3","content":"hi","timestamp":"2024-01-01T00:00:00Z"}"#;
    assert!(serde_json::from_slice::<model::Message>(raw).is_err());
}

#[test]
fn discord_member_derives_user_id() {
    // Discord member payload: `user` object, no `user_id`.
    let raw =
        br#"{"guild_id":"111111111111111111","user":{"id":"2","username":"u","discriminator":"0"},"roles":["5"]}"#;
    let m: model::Member = serde_json::from_slice(raw).unwrap();
    assert_eq!(m.user_id.get(), 2);
    assert!(m.user.is_some());
}

#[test]
fn discord_presence_derives_user_id() {
    // Discord PRESENCE_UPDATE: `user` object, no `user_id`.
    let raw =
        br#"{"user":{"id":"333333333333333333","username":"rivulet","discriminator":"0"},"guild_id":"111111111111111111","status":"online"}"#;
    let p: model::Presence = serde_json::from_slice(raw).unwrap();
    assert_eq!(p.user_id.get(), 333_333_333_333_333_333);
    assert!(p.user.is_some());
    let back = serde_json::to_value(&p).unwrap();
    let again: model::Presence = serde_json::from_value(back).unwrap();
    assert_eq!(again.user_id, p.user_id);
}
