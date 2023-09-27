use axum::routing::get;

macro_rules! resource {
	($name:tt) => {{
		#[cfg(debug_assertions)]
		{
			std::fs::read(concat!("res/", $name)).unwrap()
		}
		#[cfg(not(debug_assertions))]
		{
			include_bytes!(concat!("../res/", $name))
		}
	}};
}

macro_rules! resources {
	($($name:tt, $content_type:tt;)*) => {
		pub fn resources() -> axum::Router {
			axum::Router::new()
				$(.route(concat!("/", $name), get(|| async { ([("Content-Type", $content_type)], resource!($name)) })))*
		}
	};
}

resources! {
	"default.css", "text/css";
	"favicon.png", "image/png";
	"favicon.svg", "image/svg+xml";
}
