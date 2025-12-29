use wit_bindgen::generate;

generate!({
    path: "../../data-plane/wit/agw.wit",
    world: "plugin",
});

struct Plugin;

impl Guest for Plugin {
    fn handle_request(_req_headers: Vec<(String, String)>) -> bool {
        // Deny all requests
        false
    }
}

export!(Plugin);
