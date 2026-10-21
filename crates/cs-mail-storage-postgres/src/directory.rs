//! Protected address publication and canonical public queries.
use super::{Json, PostgresDeployment, StorageError, ingress, pricing};
use cs_mail_accounts::{
    control::AccountControl,
    directory::{DirectoryEntry, DirectoryQuery, DirectoryReference},
};
use cs_mail_application::directory::{
    self, Approach, Candidate, Context, Decision, Page, Response, SignedRequest, Store,
};
use cs_mail_primitives::CanonicalTime;
use cs_mail_security::{AuthoritySnapshot, KeyRegistry};

impl From<directory::Error> for StorageError {
    fn from(error: directory::Error) -> Self {
        match error {
            directory::Error::Invalid => {
                Self::Protocol(cs_mail_protocol::ProtocolError::PolicyInvalid)
            }
            directory::Error::Unauthorized => {
                Self::Security(cs_mail_security::SecurityError::ActorMismatch)
            }
            directory::Error::Conflict => Self::DuplicateConflict,
        }
    }
}
impl Store for PostgresDeployment {
    type Error = StorageError;
    fn transact_directory(
        &self,
        request: &SignedRequest,
        decide: impl FnOnce(Context) -> Result<Decision, StorageError>,
    ) -> Result<Response, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let address = request.address.0.to_string();
        let prior = tx
            .query_opt(
                "SELECT receipt FROM cs_directory_commands WHERE address=$1 AND operation=$2",
                &[&address, &request.operation.0.to_string()],
            )?
            .map(|r| r.get::<_, Json<Response>>(0).0);
        let mut authority = AuthoritySnapshot::new(KeyRegistry::default())?;
        ingress::add_persona_authority(&mut tx, request.address, &mut authority)?;
        let policy = pricing::current_policy(&mut tx, pricing::RowLock::Share)?;
        let current = tx
            .query_opt(
                "SELECT entry FROM cs_address_directory WHERE address=$1 FOR UPDATE",
                &[&address],
            )?
            .map(|r| r.get::<_, Json<DirectoryEntry>>(0).0);
        let classes = pricing::current_classes(&mut tx, request.address, pricing::RowLock::Update)?;
        let existing_classes = classes.clone();
        let mailbox = match &request.command {
            directory::Command::Publish { mailbox, .. } => mailbox
                .as_ref()
                .map(cs_mail_accounts::directory::MemberAddress::as_str),
            directory::Command::Inspect { .. } => None,
        };
        let mailbox_available = match mailbox {
            Some(mailbox) => tx
                .query_one(
                    "SELECT NOT EXISTS(SELECT 1 FROM cs_alias_routes WHERE address=$1) AND NOT EXISTS(SELECT 1 FROM cs_address_directory WHERE mailbox=$1 AND address<>$2)",
                    &[&mailbox, &address],
                )?
                .get::<_, bool>(0),
            None => true,
        };
        let (response, effects, record) = decide(Context {
            authority,
            current,
            classes,
            policy,
            prior,
            mailbox_available,
        })?
        .into_parts();
        if let Some(e) = effects {
            if existing_classes.as_ref() != Some(&e.classes.classes) {
                pricing::persist_classes(&mut tx, &e.classes, &e.classes.classes, e.receipt.at)?;
            }
            tx.execute("INSERT INTO cs_address_directory(address,entry,listed,search_name,search_description,mailbox) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(address) DO UPDATE SET entry=EXCLUDED.entry,listed=EXCLUDED.listed,search_name=EXCLUDED.search_name,search_description=EXCLUDED.search_description,mailbox=EXCLUDED.mailbox", &[&address, &Json(&e.entry), &(e.entry.visibility() == cs_mail_accounts::directory::DiscoveryVisibility::Listed), &e.entry.profile().display_name().to_lowercase(), &e.entry.profile().description().to_lowercase(), &e.entry.mailbox().map(cs_mail_accounts::directory::MemberAddress::as_str)])?;
        }
        if record {
            tx.execute("INSERT INTO cs_directory_commands(address,operation,request,receipt) VALUES($1,$2,$3,$4)", &[&address, &request.operation.0.to_string(), &Json(request), &Json(&response)])?;
        }
        tx.commit()?;
        Ok(response)
    }
}
fn candidate(row: &postgres::Row) -> Candidate {
    Candidate {
        entry: row.get::<_, Json<DirectoryEntry>>(0).0,
        status: row.get::<_, Json<AccountControl>>(1).0.status(),
        billing: row.get::<_, Json<cs_mail_billing::BillingAccount>>(2).0,
        classes: row
            .get::<_, Option<Json<cs_mail_protocol::pricing::RecipientRequestClasses>>>(3)
            .map(|v| v.0),
    }
}
impl PostgresDeployment {
    /// Public direct lookup includes unlisted publications, but never inactive accounts.
    /// # Errors
    /// Database or checked-restoration failure.
    pub fn directory_lookup(
        &self,
        reference: DirectoryReference,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<Option<Approach>, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let policy = pricing::current_policy(&mut tx, pricing::RowLock::Share)?;
        let (column, value) = match reference {
            DirectoryReference::Protocol(address) => ("address", address.0.to_string()),
            DirectoryReference::Mailbox(mailbox) => ("mailbox", mailbox.as_str().to_owned()),
        };
        // The column is selected solely by the typed reference, never caller text.
        let sql = format!(
            "SELECT d.entry,a.control,b.record,c.classes FROM cs_address_directory d JOIN cs_persona_owners o ON o.identity=d.address JOIN cs_product_accounts a ON a.id=o.account JOIN cs_billing_accounts b ON b.id=a.billing LEFT JOIN cs_recipient_request_classes c ON c.recipient=d.address WHERE d.{column}=$1"
        );
        let row = tx.query_opt(&sql, &[&value])?;
        let at = clock();
        let result = row.and_then(|r| directory::approach(candidate(&r), policy.as_ref(), at));
        tx.commit()?;
        Ok(result)
    }
    /// Bounded canonical search. Ordering is normalized public name then address;
    /// pagination is a live view, not a frozen population snapshot.
    /// # Errors
    /// Database or checked-restoration failure.
    pub fn directory_search(
        &self,
        query: &DirectoryQuery,
        clock: impl FnOnce() -> CanonicalTime,
    ) -> Result<Page, StorageError> {
        let mut client = self.client.lock().map_err(|_| StorageError::LockPoisoned)?;
        let mut tx = client.transaction()?;
        ingress::lock_receipt_order(&mut tx)?;
        let policy = pricing::current_policy(&mut tx, pricing::RowLock::Share)?;
        let rows = tx.query("SELECT d.entry,a.control,b.record,c.classes FROM cs_address_directory d JOIN cs_persona_owners o ON o.identity=d.address JOIN cs_product_accounts a ON a.id=o.account JOIN cs_billing_accounts b ON b.id=a.billing LEFT JOIN cs_recipient_request_classes c ON c.recipient=d.address WHERE d.listed AND a.control->>'status'='Active' AND (strpos(d.search_name,$1)>0 OR strpos(d.search_description,$1)>0 OR strpos(d.address,$1)>0 OR strpos(d.mailbox,$1)>0) ORDER BY d.search_name COLLATE \"C\",d.address COLLATE \"C\" LIMIT 21 OFFSET $2", &[&query.text().to_lowercase(), &i64::from(query.offset())])?;
        let at = clock();
        let mut entries = rows
            .iter()
            .map(candidate)
            .filter(|c| directory::searchable(&c.entry, query))
            .filter_map(|c| directory::approach(c, policy.as_ref(), at))
            .collect::<Vec<_>>();
        let next_offset =
            (entries.len() > 20 && query.offset() <= 9980).then_some(query.offset() + 20);
        entries.truncate(20);
        tx.commit()?;
        Ok(Page {
            entries,
            next_offset,
        })
    }
}
