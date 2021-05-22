#[macro_use]
mod logger;

use serenity::{
    async_trait,
    client::{bridge::gateway::GatewayIntents, Client, Context, EventHandler},
    model::{
        guild::Member,
        id::{GuildId, UserId},
        prelude::{Activity, OnlineStatus, Ready, User},
    },
    prelude::TypeMapKey,
};

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::interval;

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: Ready) {
        ctx.set_presence(
            Some(Activity::playing("Cupcakes are good :3")),
            OnlineStatus::Invisible,
        )
        .await;

        info!("Ready!");
    }

    async fn guild_member_removal(
        &self,
        ctx: Context,
        guild_id: GuildId,
        kicked_user: User,
        _member_data_if_available: Option<Member>,
    ) {
        let mut data = ctx.data.write().await;
        let rate_limit = data.get_mut::<RateLimit>().unwrap();

        match guild_id
            .audit_logs(&ctx.http, None, None, None, Some(1))
            .await
        {
            Ok(n) => {
                let entries = n.entries.values().collect::<Vec<_>>();
                let tag = entries[0]
                    .user_id
                    .to_user(&ctx.http)
                    .await
                    .map(|x| x.tag())
                    .unwrap_or("unknown-user".to_string());

                let to_notify = vec![807224123691761704, 805035493627920385];

                match rate_limit.get_mut(&entries[0].user_id) {
                    Some(n) => {
                        *n += 1;

                        debug!("user = {}; count = {}", entries[0].user_id.0, *n);

                        if *n < 5 {
                            let message = format!(
                                "<@{}> ({}) has just kicked/banned <@{}> ({})",
                                entries[0].user_id.0,
                                tag,
                                kicked_user.id.0,
                                kicked_user.tag(),
                            );

                            for id in to_notify.clone() {
                                send_message(&ctx, id, &message).await;
                            }

                            return;
                        }

                        drop(rate_limit);
                        drop(data);

                        debug!("user = {}; removing admin roles", entries[0].user_id.0);

                        let message = format!(
                            "<@{}> ({}) has had their admin role removed",
                            entries[0].user_id.0, tag,
                        );

                        for id in to_notify.clone() {
                            send_message(&ctx, id, &message).await;
                        }

                        match guild_id.member(&ctx.http, entries[0].user_id).await {
                            Ok(mut n) => {
                                for role in vec![834782660148592700, 834912308169015387] {
                                    match n.remove_role(&ctx.http, role).await {
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
                        rate_limit.insert(entries[0].user_id, 1);

                        drop(rate_limit);
                        drop(data);

                        debug!("user = {}; count = 1", entries[0].user_id.0);

                        let message = format!(
                            "<@{}> ({}) has just kicked/banned <@{}> ({})",
                            entries[0].user_id.0,
                            tag,
                            kicked_user.id.0,
                            kicked_user.tag(),
                        );

                        for id in to_notify.clone() {
                            send_message(&ctx, id, &message).await;
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

struct RateLimit;

impl TypeMapKey for RateLimit {
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

    client
        .data
        .write()
        .await
        .insert::<RateLimit>(HashMap::new());

    {
        let data = client.data.clone();

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(600));

            loop {
                interval.tick().await;

                let mut data = data.write().await;
                let rate_limit = data.get_mut::<RateLimit>().unwrap();

                debug!("600 seconds elapsed - clearing rate limit map");

                rate_limit.clear();
            }
        });
    }

    if let Err(why) = client.start().await {
        eprintln!("An error occurred while running the client: {:?}", why);
    }
}
