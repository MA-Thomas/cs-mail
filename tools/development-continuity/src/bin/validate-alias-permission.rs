//! Read-only validation for the explicit schema-41 preservation copy.
use cs_mail_accounts::aliases::{Alias, AliasPortfolio};
use cs_mail_application::aliases::DeliveredRoute;
use postgres::{Client, NoTls, types::Json};
use std::collections::BTreeMap;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::args().nth(1).ok_or("explicit local destination URL required")?;
    if !url.starts_with("postgresql://localhost/") { return Err("local destination required".into()); }
    let mut db = Client::connect(&url, NoTls)?;
    let mut owners = BTreeMap::<_, BTreeMap<_, _>>::new();
    for row in db.query("SELECT id,owner,address,record FROM cs_alias_routes ORDER BY id", &[])? {
        let a: Alias = row.try_get::<_, Json<Alias>>(3)?.0;
        if a.id().value().to_string() != row.get::<_,String>(0) || a.owner().0.to_string() != row.get::<_,String>(1) || a.address().as_str() != row.get::<_,String>(2) { return Err("substituted alias".into()); }
        owners.entry(a.owner()).or_default().insert(a.id(),a);
    }
    for (owner, aliases) in &owners { AliasPortfolio::new(*owner, aliases)?; }
    for row in db.query("SELECT evidence FROM cs_alias_deliveries", &[])? { let _: DeliveredRoute = row.try_get::<_,Json<DeliveredRoute>>(0)?.0; }
    println!("PASS checked address portfolios, independent permission records and receiving evidence; read-only validation");
    Ok(())
}
