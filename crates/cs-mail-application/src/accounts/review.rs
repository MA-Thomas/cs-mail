//! Operator review of enrollments the identity service holds because the applicant's
//! evidence overlaps another enrollment's. cs-mail authenticates the operator and
//! forwards the review under its product key; the identity service records each
//! decision once, with the operator and the command that authorized it.
use cs_mail_operations::{
    OperationsError, OperatorAuthority, ReviewCommand, SignedOperatorCommand,
};
use identity_contract::{
    self as contract, IdentityClient, Request, Response, SignedRequest, evidence::ReviewCase,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReviewOutcome {
    Open(Vec<ReviewCase>),
    Resolved(ReviewCase),
}
#[derive(Debug)]
pub enum ReviewError {
    Operations(OperationsError),
    Contract(contract::Error),
    Client(contract::ClientError),
}
impl From<contract::Error> for ReviewError {
    fn from(e: contract::Error) -> Self {
        Self::Contract(e)
    }
}

/// Authenticates an operator's review command and forwards it to the identity service
/// for `product`. No local locks are held.
/// # Errors
/// `Operations(Unauthorized)` for a command the deployment's operators did not sign;
/// the identity service's refusal, or a transport failure, otherwise.
pub fn review_enrollments(
    authority: &OperatorAuthority,
    signed: &SignedOperatorCommand<ReviewCommand>,
    product: &str,
    client: &impl IdentityClient,
    product_secret: &[u8; 32],
) -> Result<ReviewOutcome, ReviewError> {
    let verified = authority.verify(signed).map_err(ReviewError::Operations)?;
    let request = match verified.command() {
        ReviewCommand::Open => Request::Reviews,
        ReviewCommand::Resolve { case, resolution } => Request::ResolveReview {
            case: case.clone(),
            resolution: *resolution,
            reviewer: format!("operator:{}", verified.operator().0),
            authorization: verified.digest().iter().fold(String::new(), |mut hex, b| {
                use std::fmt::Write as _;
                let _ = write!(hex, "{b:02x}");
                hex
            }),
        },
    };
    let request = SignedRequest::sign(product.into(), request, product_secret)?;
    match (
        verified.command(),
        client.call(&request).map_err(ReviewError::Client)?,
    ) {
        (ReviewCommand::Open, Response::Reviews(cases)) => Ok(ReviewOutcome::Open(cases)),
        (ReviewCommand::Resolve { .. }, Response::ReviewResolved(case)) => {
            Ok(ReviewOutcome::Resolved(*case))
        }
        (_, Response::Rejected(error)) => Err(error.into()),
        _ => Err(contract::Error::Context.into()),
    }
}
