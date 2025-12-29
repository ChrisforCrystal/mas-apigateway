use wit_bindgen::generate;

generate!({
    path: "../../data-plane/wit/agw.wit",
    world: "plugin",
});

struct Plugin;

impl Guest for Plugin {
    fn handle_request(req_headers: Vec<(String, String)>) -> bool {
        // 1. Get X-DB-Type header
        let db_type_str = req_headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == "x-db-type")
            .map(|(_, v)| v.as_str())
            .unwrap_or("postgres"); // Default to postgres

        // 2. Determine DB Type and Connection Name
        let (db_enum, conn_name) = match db_type_str {
            "mysql" => (mas::agw::database::DbType::Mysql, "products-mysql"),
            _ => (mas::agw::database::DbType::Postgres, "users-pg"),
        };

        // 3. Construct SQL
        let sql = match db_enum {
            mas::agw::database::DbType::Mysql => "SELECT name FROM products LIMIT 1",
            mas::agw::database::DbType::Postgres => "SELECT username FROM users LIMIT 1",
        };

        // 4. Execute Query via WIT
        let result = mas::agw::database::query(db_enum, conn_name, sql);

        match result {
            Ok(Ok(json)) => {
                // Log the result
                mas::agw::logging::log(
                    mas::agw::logging::Level::Debug,
                    &format!("DB Query Result: {}", json),
                );
            }
            Ok(Err(e)) => {
                mas::agw::logging::log(
                    mas::agw::logging::Level::Error,
                    &format!("DB Error: {}", e),
                );
            }
            Err(e) => {
                mas::agw::logging::log(
                    mas::agw::logging::Level::Error,
                    &format!("Host Call Error: {}", e),
                );
            }
        }

        true // Allow request
    }
}

export!(Plugin);
