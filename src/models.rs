use sqlx::{PgPool, Pool};

pub struct Account {
    pub id: i32,
    pub twitter_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub session_key: Option<String>,
    pub owned_by: Option<i32>,
}

impl Account {
    pub async fn owning(&self, conn: PgPool) -> Result<Vec<Account>, sqlx::Error> {
        let rec = sqlx::query!(
            "select * from accounts where owned_by = $1 order by id",
            self.id
        )
        .fetch_all(&conn)
        .await?;

        let accounts = rec
            .into_iter()
            .map(|r| Account {
                id: r.id,
                twitter_id: r.twitter_id,
                access_token: r.access_token,
                refresh_token: r.refresh_token,
                session_key: r.session_key,
                owned_by: r.owned_by,
            })
            .collect();
        Ok(accounts)
    }
}
