mod parse;
use parse::Parse;
mod helpers;
use helpers::new_ds;
use surrealdb::Result;
use surrealdb::dbs::Session;
use surrealdb::err::Error;
use surrealdb::sql::SqlValue;

#[tokio::test]
async fn future_function_arguments() -> Result<()> {
	let sql = "
		UPSERT future:test SET
			a = 'test@surrealdb.com',
			b = <future> { 'test@surrealdb.com' },
			x = 'a-' + parse::email::user(a),
			y = 'b-' + parse::email::user(b)
		;
	";
	let dbs = new_ds().await?;
	let ses = Session::owner().with_ns("test").with_db("test");
	let res = &mut dbs.execute(sql, &ses, None).await?;
	assert_eq!(res.len(), 1);
	//
	let tmp = res.remove(0).result?;
	let val = SqlValue::parse(
		"[
			{
				a: 'test@surrealdb.com',
				b: 'test@surrealdb.com',
				id: future:test,
				x: 'a-test',
				y: 'b-test',
			}
		]",
	)
	.into();
	assert_eq!(tmp, val);
	//
	Ok(())
}

#[tokio::test]
async fn future_disabled() -> Result<()> {
	let sql = "
	    OPTION FUTURES = false;
		<future> { 123 };
	";
	let dbs = new_ds().await?;
	let ses = Session::owner().with_ns("test").with_db("test");
	let res = &mut dbs.execute(sql, &ses, None).await?;
	assert_eq!(res.len(), 1);
	//
	let tmp = res.remove(0).result?;
	let val = SqlValue::parse("<future> { 123 }").into();
	assert_eq!(tmp, val);
	//
	Ok(())
}

#[tokio::test]
#[ignore]
async fn concurrency() -> Result<()> {
	// cargo test --package surrealdb --test future --features kv-mem --release -- concurrency --nocapture

	const MILLIS: usize = 50;

	// If all futures complete in less than double `MILLIS`, then they must have executed
	// concurrently. Otherwise, some executed sequentially.
	const TIMEOUT: usize = MILLIS * 19 / 10;

	/// Returns a query that will execute `count` futures that each wait for `millis`
	fn query(count: usize, millis: usize) -> String {
		// TODO: Find a simpler way to trigger the concurrent future case.
		format!(
			"SELECT foo FROM [[{}]] TIMEOUT {TIMEOUT}ms;",
			(0..count)
				.map(|i| format!("<future>{{[sleep({millis}ms), {{foo: {i}}}]}}"))
				.collect::<Vec<_>>()
				.join(", ")
		)
	}

	/// Returns `true` if `limit` futures are concurrently executed.
	async fn test_limit(limit: usize) -> Result<bool> {
		let sql = query(limit, MILLIS);
		let dbs = new_ds().await?;
		let ses = Session::owner().with_ns("test").with_db("test");
		let res = dbs.execute(&sql, &ses, None).await;

		match res {
			Err(err) => {
				if matches!(err.downcast_ref(), Some(Error::QueryTimedout)) {
					Ok(false)
				} else {
					Err(err)
				}
			}
			Ok(res) => {
				assert_eq!(res.len(), 1);

				let res = res.into_iter().next().unwrap();

				let elapsed = res.time.as_millis() as usize;

				Ok(elapsed < TIMEOUT)
			}
		}
	}

	// Diagnostics.
	/*
	for i in (1..=80).step_by(8) {
		println!("{i} futures => {}", test_limit(i).await?);
	}
	*/

	assert!(test_limit(3).await?);

	// Too slow to *parse* query in debug mode.
	#[cfg(not(debug_assertions))]
	assert!(!test_limit(64 /* surrealdb::cnf::MAX_CONCURRENT_TASKS */ + 1).await?);

	Ok(())
}
