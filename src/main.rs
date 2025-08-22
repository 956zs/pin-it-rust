use anyhow::Result;
use dashmap::DashMap;
use dotenv::dotenv;
use once_cell::sync::Lazy;
use serenity::{
    all::{
        ChannelId, Context, EventHandler, GatewayIntents, Message, MessageId, Reaction,
        ReactionType, Ready, UserId,
    },
    async_trait, Client,
};
use std::{
    collections::HashSet,
    env,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::time::{interval, sleep};
use tracing::{error, info, warn};

// Pre-computed number emojis for O(1) lookup
static NUMBER_EMOJIS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "<:1_:1404868671704272906>",
        "<:2_:1404868687969910986>",
        "<:3_:1404868696123375757>",
        "<:4_:1404868709150888167>",
        "<:5_:1404868718064042004>",
        "<:6_:1404868725416661064>",
        "<:7_:1404868732400173148>",
        "<:8_:1404868741807996978>",
        "<:9_:1404868751387660428>",
        "<:10:1404868763710652547>",
    ]
});

const CHECKMARK_EMOJI: &str = "✅";
const SLASH_EMOJI: &str = "<:slash:1404872667189743697>";
const PIN_COOLDOWN_SECS: u64 = 5;
const CLEANUP_INTERVAL_SECS: u64 = 300; // 5 minutes
const SESSION_MAX_AGE_SECS: u64 = 3600; // 1 hour

#[derive(Debug, Clone)]
struct VotingSession {
    target_message_id: MessageId,
    target_channel_id: ChannelId,
    voters: HashSet<UserId>,
    vote_count: Arc<AtomicU32>,
    created_at: Instant,
}

impl VotingSession {
    fn new(target_message_id: MessageId, target_channel_id: ChannelId) -> Self {
        Self {
            target_message_id,
            target_channel_id,
            voters: HashSet::new(),
            vote_count: Arc::new(AtomicU32::new(0)),
            created_at: Instant::now(),
        }
    }

    fn add_vote(&mut self, user_id: UserId) -> bool {
        if self.voters.insert(user_id) {
            self.vote_count.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    fn remove_vote(&mut self, user_id: UserId) -> bool {
        if self.voters.remove(&user_id) {
            self.vote_count.fetch_sub(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    fn get_vote_count(&self) -> u32 {
        self.vote_count.load(Ordering::Relaxed)
    }

    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > Duration::from_secs(SESSION_MAX_AGE_SECS)
    }
}

struct BotData {
    voting_sessions: DashMap<MessageId, VotingSession>,
    pin_cooldowns: DashMap<ChannelId, Instant>,
    confirm_cap: u32,
}

impl BotData {
    fn new(confirm_cap: u32) -> Self {
        Self {
            voting_sessions: DashMap::new(),
            pin_cooldowns: DashMap::new(),
            confirm_cap,
        }
    }

    fn get_number_emoji(&self, num: u32) -> Option<&'static str> {
        if num == 0 || num > 10 {
            return None;
        }
        NUMBER_EMOJIS.get((num - 1) as usize).copied()
    }

    async fn pin_message_safely(
        &self,
        ctx: &Context,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> bool {
        let now = Instant::now();

        // Check rate limit
        if let Some(last_pin) = self.pin_cooldowns.get(&channel_id) {
            if now.duration_since(*last_pin) < Duration::from_secs(PIN_COOLDOWN_SECS) {
                warn!("Pin rate limited for channel {}", channel_id);
                return false;
            }
        }

        match ctx.http.pin_message(channel_id, message_id, None).await {
            Ok(_) => {
                self.pin_cooldowns.insert(channel_id, now);
                info!(
                    "Successfully pinned message {} in channel {}",
                    message_id, channel_id
                );
                true
            }
            Err(e) => {
                error!("Failed to pin message {}: {}", message_id, e);
                false
            }
        }
    }

    fn cleanup_expired_sessions(&self) {
        let mut removed_count = 0;
        self.voting_sessions.retain(|_, session| {
            if session.is_expired() {
                removed_count += 1;
                false
            } else {
                true
            }
        });

        if removed_count > 0 {
            info!("Cleaned up {} expired voting sessions", removed_count);
        }
    }
}

struct Handler {
    data: Arc<BotData>,
}

impl Handler {
    fn new(confirm_cap: u32) -> Self {
        Self {
            data: Arc::new(BotData::new(confirm_cap)),
        }
    }

    fn start_cleanup_task(&self) {
        let data = Arc::clone(&self.data);
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(CLEANUP_INTERVAL_SECS));
            loop {
                interval.tick().await;
                data.cleanup_expired_sessions();
            }
        });
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Bot {} is ready!", ready.user.name);
        self.start_cleanup_task();
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Ignore own messages and messages without references
        if msg.author.bot || msg.referenced_message.is_none() {
            return;
        }

        // Check if bot is mentioned
        let current_user_id = ctx.cache.current_user().id;
        if !msg.content.starts_with(&format!("<@{}>", current_user_id))
            && !msg.content.starts_with(&format!("<@!{}>", current_user_id))
        {
            return;
        }

        let target_msg = match msg.referenced_message.as_ref() {
            Some(target) => target,
            None => return,
        };

        // If confirm_cap is 0, pin immediately
        if self.data.confirm_cap == 0 {
            self.data
                .pin_message_safely(&ctx, msg.channel_id, target_msg.id)
                .await;
            return;
        }

        // Create voting session
        let session = VotingSession::new(target_msg.id, msg.channel_id);
        self.data.voting_sessions.insert(msg.id, session);

        // Add reactions with error handling
        let reactions = vec![
            CHECKMARK_EMOJI,
            SLASH_EMOJI,
            self.data
                .get_number_emoji(self.data.confirm_cap)
                .unwrap_or("❓"),
        ];

        for &emoji in &reactions {
            if let Err(e) = msg
                .react(&ctx.http, ReactionType::Unicode(emoji.to_string()))
                .await
            {
                warn!("Failed to add reaction {}: {}", emoji, e);
                // For custom emojis, try parsing them
                if emoji.starts_with('<') {
                    if let Ok(custom_emoji) = emoji.parse::<ReactionType>() {
                        if let Err(e2) = msg.react(&ctx.http, custom_emoji).await {
                            warn!("Failed to add custom reaction {}: {}", emoji, e2);
                        }
                    }
                }
            }
            // Small delay to avoid rate limits
            sleep(Duration::from_millis(100)).await;
        }
    }

    async fn reaction_add(&self, ctx: Context, reaction: Reaction) {
        // Ignore bot reactions
        if let Ok(user) = reaction.user(&ctx.http).await {
            if user.bot {
                return;
            }
        } else {
            return;
        }

        // Only handle checkmark reactions
        if !matches!(&reaction.emoji, ReactionType::Unicode(s) if s == CHECKMARK_EMOJI) {
            return;
        }

        let user_id = match reaction.user_id {
            Some(id) => id,
            None => return,
        };

        // Get and update voting session
        if let Some(mut session_entry) = self.data.voting_sessions.get_mut(&reaction.message_id) {
            let session = session_entry.value_mut();

            if session.add_vote(user_id) {
                let current_votes = session.get_vote_count();
                info!(
                    "Vote added by {} for message {}. Count: {}",
                    user_id, reaction.message_id, current_votes
                );

                // Check if threshold reached
                if current_votes >= self.data.confirm_cap {
                    let target_message_id = session.target_message_id;
                    let target_channel_id = session.target_channel_id;

                    // Drop the session entry to release the lock
                    drop(session_entry);

                    let success = self
                        .data
                        .pin_message_safely(&ctx, target_channel_id, target_message_id)
                        .await;

                    if success {
                        // Clean up the session
                        self.data.voting_sessions.remove(&reaction.message_id);
                    }
                }
            }
        }
    }

    async fn reaction_remove(&self, _ctx: Context, reaction: Reaction) {
        // Ignore bot reactions
        if let Ok(user) = reaction.user(&_ctx.http).await {
            if user.bot {
                return;
            }
        } else {
            return;
        }

        // Only handle checkmark reactions
        if !matches!(&reaction.emoji, ReactionType::Unicode(s) if s == CHECKMARK_EMOJI) {
            return;
        }

        let user_id = match reaction.user_id {
            Some(id) => id,
            None => return,
        };

        // Update voting session
        if let Some(mut session_entry) = self.data.voting_sessions.get_mut(&reaction.message_id) {
            let session = session_entry.value_mut();

            if session.remove_vote(user_id) {
                let current_votes = session.get_vote_count();
                info!(
                    "Vote removed by {} for message {}. Count: {}",
                    user_id, reaction.message_id, current_votes
                );
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Load environment variables
    dotenv().ok();

    let token = env::var("TOKEN").expect("TOKEN environment variable not set");

    let confirm_cap: u32 = env::var("CONFIRM_CAP")
        .unwrap_or_else(|_| "3".to_string())
        .parse()
        .expect("CONFIRM_CAP must be a valid number");

    if confirm_cap > 10 {
        panic!("CONFIRM_CAP must be between 0 and 10");
    }

    info!("Starting bot with confirm_cap: {}", confirm_cap);

    // Set gateway intents - minimal for performance
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS
        | GatewayIntents::MESSAGE_CONTENT;

    // Create client
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler::new(confirm_cap))
        .await?;

    // Start the client
    if let Err(e) = client.start().await {
        error!("Client error: {}", e);
    }

    Ok(())
}
