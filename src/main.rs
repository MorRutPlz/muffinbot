#[macro_use]
mod logger;

use serenity::{
    async_trait,
    client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler},
    model::{
        channel::{GuildChannel, Message},
        guild::{Action, ActionChannel, ActionMember, Member},
        id::{ChannelId, GuildId, UserId},
        prelude::{Activity, OnlineStatus, Ready, User},
    },
    prelude::TypeMapKey,
};

use std::time::{Duration, UNIX_EPOCH};
use std::{collections::HashMap, time::SystemTime};
use tokio::time::interval;

static ADMIN_ROLES: [u64; 2] = [834782660148592700, 834912308169015387];
static TO_NOTIFY: [u64; 2] = [807224123691761704, 805035493627920385];

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: Ready) {
        ctx.set_presence(
            Some(Activity::playing("DM me to post a confession! >:3")),
            OnlineStatus::Online,
        )
        .await;

        info!("Ready!");
    }

    async fn message(&self, ctx: Context, new_message: Message) {
        if new_message.is_private() && !new_message.is_own(&ctx.cache).await {
            let mut data = ctx.data.write().await;
            let rate_limit = data.get_mut::<ConfessionRateLimit>().unwrap();

            let current = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            match rate_limit.get(&new_message.author.id) {
                Some(last_sent) => {
                    if current - *last_sent < 1800 {
                        let mut message = String::new();
                        message.push_str("You can only send one confession every 30 minutes! ");
                        message.push_str("Time remaining: ");
                        message.push_str(
                            &humantime::format_duration(Duration::from_secs(
                                1800 - ((current - last_sent) as u64),
                            ))
                            .to_string(),
                        );

                        match new_message.reply(&ctx.http, &message).await {
                            Ok(_) => {}
                            Err(e) => println!("failed to send rate limit message: {}", e),
                        }

                        return;
                    }
                }
                None => {}
            }

            let message = new_message.content_safe(&ctx.cache).await;

            match ChannelId(846029848052629574)
                .send_message(&ctx.http, |m| {
                    m.content(format!("**CONFESSION**\n{}", message))
                })
                .await
            {
                Ok(message) => {
                    rate_limit.insert(new_message.author.id, message.timestamp.timestamp());

                    match new_message.reply(&ctx.http, "Sent confession <3").await {
                        Ok(_) => {}
                        Err(e) => println!("failed to send confirm message: {}", e),
                    };
                }
                Err(e) => println!("failed to send message: {}", e),
            }
        }
    }

    async fn channel_create(&self, ctx: Context, channel: &GuildChannel) {
        let mut data = ctx.data.write().await;
        let rate_limit = data.get_mut::<ChannelCreateRateLimit>().unwrap();

        match channel
            .guild_id
            .audit_logs(&ctx.http, None, None, None, Some(5))
            .await
        {
            Ok(n) => {
                let mut entries = n.entries.values().collect::<Vec<_>>();
                entries.sort_by(|a, b| {
                    snowflake_to_time(a.id.0)
                        .partial_cmp(&snowflake_to_time(b.id.0))
                        .unwrap()
                });

                entries.reverse();

                let mut entries = entries.into_iter();

                let entry = loop {
                    match entries.next() {
                        Some(entry) => match entry.action {
                            Action::Channel(ActionChannel::Create) => {
                                if entry.target_id.unwrap() == channel.id.0 {
                                    break entry;
                                }
                            }
                            _ => {}
                        },
                        None => return,
                    }
                };

                let tag = entry
                    .user_id
                    .to_user(&ctx.http)
                    .await
                    .map(|x| x.tag())
                    .unwrap_or("unknown-user".to_string());

                match rate_limit.get_mut(&entry.user_id) {
                    Some(n) => {
                        *n += 1;

                        if *n < 10 {
                            let message = format!(
                                "<@{}> ({}) has just created channel ({})",
                                entry.user_id.0,
                                tag,
                                channel.name(),
                            );

                            for id in TO_NOTIFY.iter() {
                                send_message(&ctx, *id, &message).await;
                            }

                            return;
                        }

                        drop(rate_limit);
                        drop(data);

                        let message = format!(
                            "<@{}> ({}) has had their admin role removed",
                            entry.user_id.0, tag,
                        );

                        for id in TO_NOTIFY.iter() {
                            send_message(&ctx, *id, &message).await;
                        }

                        match channel.guild_id.member(&ctx.http, entry.user_id).await {
                            Ok(mut n) => {
                                for role in ADMIN_ROLES.iter() {
                                    match n.remove_role(&ctx.http, *role).await {
                                        Ok(_) => {}
                                        Err(e) => {
                                            warn!("Error removing role: {}", e)
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error getting member: {}", e);
                            }
                        }
                    }
                    None => {
                        rate_limit.insert(entry.user_id, 1);

                        drop(rate_limit);
                        drop(data);

                        let message = format!(
                            "<@{}> ({}) has just created channel ({})",
                            entry.user_id.0,
                            tag,
                            channel.name(),
                        );

                        for id in TO_NOTIFY.iter() {
                            send_message(&ctx, *id, &message).await;
                        }
                    }
                }
            }
            Err(e) => error!("Error getting audit logs: {}", e),
        }
    }

    async fn guild_member_removal(
        &self,
        ctx: Context,
        guild_id: GuildId,
        kicked_user: User,
        _member_data_if_available: Option<Member>,
    ) {
        let mut data = ctx.data.write().await;
        let rate_limit = data.get_mut::<KickBanRateLimit>().unwrap();

        match guild_id
            .audit_logs(&ctx.http, None, None, None, Some(5))
            .await
        {
            Ok(n) => {
                let mut entries = n.entries.values().collect::<Vec<_>>();
                entries.sort_by(|a, b| {
                    snowflake_to_time(a.id.0)
                        .partial_cmp(&snowflake_to_time(b.id.0))
                        .unwrap()
                });

                entries.reverse();

                let mut entries = entries.into_iter();

                let entry = loop {
                    match entries.next() {
                        Some(entry) => match entry.action {
                            Action::Member(ActionMember::Kick)
                            | Action::Member(ActionMember::BanAdd) => {
                                if entry.target_id.unwrap() == kicked_user.id.0 {
                                    break entry;
                                }
                            }
                            _ => {}
                        },
                        None => return,
                    }
                };

                match entry.action {
                    Action::Member(ActionMember::Kick) | Action::Member(ActionMember::BanAdd) => {
                        if entry.target_id.unwrap() != kicked_user.id.0 {
                            return;
                        }
                    }
                    _ => return,
                }

                let tag = entry
                    .user_id
                    .to_user(&ctx.http)
                    .await
                    .map(|x| x.tag())
                    .unwrap_or("unknown-user".to_string());

                match rate_limit.get_mut(&entry.user_id) {
                    Some(n) => {
                        *n += 1;

                        debug!("user = {}; count = {}", entry.user_id.0, *n);

                        if *n < 5 {
                            let message = format!(
                                "<@{}> ({}) has just kicked/banned <@{}> ({})",
                                entry.user_id.0,
                                tag,
                                kicked_user.id.0,
                                kicked_user.tag(),
                            );

                            for id in TO_NOTIFY.iter() {
                                send_message(&ctx, *id, &message).await;
                            }

                            return;
                        }

                        drop(rate_limit);
                        drop(data);

                        debug!("user = {}; removing admin roles", entry.user_id.0);

                        let message = format!(
                            "<@{}> ({}) has had their admin role removed",
                            entry.user_id.0, tag,
                        );

                        for id in TO_NOTIFY.iter() {
                            send_message(&ctx, *id, &message).await;
                        }

                        match guild_id.member(&ctx.http, entry.user_id).await {
                            Ok(mut n) => {
                                for role in ADMIN_ROLES.iter() {
                                    match n.remove_role(&ctx.http, *role).await {
                                        Ok(_) => {}
                                        Err(e) => {
                                            warn!("Error removing role: {}", e)
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error getting member: {}", e);
                            }
                        }
                    }
                    None => {
                        rate_limit.insert(entry.user_id, 1);

                        drop(rate_limit);
                        drop(data);

                        debug!("user = {}; count = 1", entry.user_id.0);

                        let message = format!(
                            "<@{}> ({}) has just kicked/banned <@{}> ({})",
                            entry.user_id.0,
                            tag,
                            kicked_user.id.0,
                            kicked_user.tag(),
                        );

                        for id in TO_NOTIFY.iter() {
                            send_message(&ctx, *id, &message).await;
                        }
                    }
                }
            }
            Err(e) => error!("Error getting audit logs: {}", e),
        }
    }
}

async fn send_message(ctx: &Context, user_id: u64, content: &str) {
    match UserId(user_id).create_dm_channel(&ctx.http).await {
        Ok(n) => match n.send_message(&ctx.http, |m| m.content(content)).await {
            Ok(_) => {}
            Err(_) => {}
        },
        Err(_) => {}
    }
}

struct ChannelCreateRateLimit;
struct ConfessionRateLimit;
struct KickBanRateLimit;

impl TypeMapKey for ChannelCreateRateLimit {
    type Value = HashMap<UserId, usize>;
}

impl TypeMapKey for ConfessionRateLimit {
    type Value = HashMap<UserId, i64>;
}

impl TypeMapKey for KickBanRateLimit {
    type Value = HashMap<UserId, usize>;
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut client = Client::builder(std::env::var("TOKEN").unwrap())
        .event_handler(Handler)
        .intents(GatewayIntents::all())
        .await
        .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ChannelCreateRateLimit>(HashMap::new());
        data.insert::<ConfessionRateLimit>(HashMap::new());
        data.insert::<KickBanRateLimit>(HashMap::new());
    }

    {
        let data = client.data.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(600));

            loop {
                interval.tick().await;

                let mut data = data.write().await;
                let rate_limit = data.get_mut::<KickBanRateLimit>().unwrap();

                debug!("600 seconds elapsed - clearing rate limit map");

                rate_limit.clear();
            }
        });
    }

    if let Err(why) = client.start().await {
        eprintln!("An error occurred while running the client: {:?}", why);
    }
}

fn snowflake_to_time(snowflake: u64) -> u64 {
    (snowflake >> 22) + 1420070400000
}
