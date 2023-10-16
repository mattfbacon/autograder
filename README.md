# Autograder

A web application that automatically evaluates submissions with test cases. It's similar to judging programs used for competitive programming, but is oriented more toward CS labs and problem solving activities.

## Features

- Doesn't use any JS
- Easy backups: just copy one file
- Custom judgers
- Password reset by email
- Supports Python, C, C++, Java, and Rust for submissions (and is extensible in code)
- Sandboxes submissions with Docker, providing a consistent and secure environment and the newest compilers and interpreters even on old hosts

## Deployment

First, build the binary. We use SQLX for compile-time checked queries, but we maintain offline query data, so you should set `SQLX_OFFLINE=1` in the environment variables to avoid having to initialize the database yourself. The application binary will handle initializing the database and running migrations.

(Tip: on x86_64, build for the MUSL target to avoid issues with GLIBC versioning.)

Next, on the server, create a directory with the following contents:

- `autograder`: The program binary.
- `config.toml`: Your configuration. See `config.toml.example` for an annotated example config.
- `sandbox`: Copied from the repository and used at runtime to build the docker container.
- `res`: Optional; copied from the repository. These will be used for nginx static resource acceleration.

I recommend creating a dedicated user and group for the autograder.

Next, you can use the following systemd unit or its equivalent:

```systemd
[Unit]
Description=autograder server
After=network-online.target docker.service
Requires=docker.service
Wants=network-online.target

[Service]
# So nginx can access our socket.
UMask=007
LimitNOFILE=65536
Type=simple
ExecStart=/opt/autograder/autograder
WorkingDirectory=/opt/autograder
# In our config we wrote `/run/autograder/http.sock` and this directive provides that directory for us.
RuntimeDirectory=autograder
User=autograder
Group=autograder

PrivateDevices=yes
# docker needs to be able to access our temporary directories.
PrivateTmp=no
ProtectSystem=strict
# We need:
# - `/opt/autograder` to modify our database.
# - `/var/run/docker.sock` to run the sandbox.
# - `/tmp` to create docker-accessible volumes containing submission info.
ReadWritePaths=/opt/autograder /var/run/docker.sock /tmp
ProtectKernelTunables=yes
ProtectControlGroups=yes
AmbientCapabilities=
CapabilityBoundingSet=
NoNewPrivileges=yes
# We have configured the server to bind to a unix socket, so that's all it needs.
# If you change it to bind to a TCP socket, `AF_UNIX` is still required in order to interact with docker.
RestrictAddressFamilies=AF_UNIX
ProtectProc=noaccess
RestrictNamespaces=yes
RestrictRealtime=yes
RemoveIPC=yes
ProtectHostname=yes
ProtectClock=yes
ProtectKernelLogs=yes
ProtectKernelModules=yes
MemoryDenyWriteExecute=yes
LockPersonality=yes
DevicePolicy=closed
SystemCallArchitectures=native
SystemCallFilter=@system-service ~@privileged ~@resources
RestrictSUIDSGID=yes
PrivateUsers=yes
ProtectHome=yes
PrivateNetwork=no

[Install]
WantedBy=multi-user.target
```

Finally, set up nginx or a similar reverse proxy in front of the application server, to provide TLS and optionally to accelerate requests for static resources. You can use the following config:

```nginx
server {
	server_name YOUR_SERVER_NAME;
	root /opt/autograder;

	location / {
		proxy_pass http://unix:/run/autograder/http.sock;
		proxy_set_header Host $host;
	}

	location /res {
		expires 1d;
		add_header Cache-Control "public";
		try_files $uri $uri/ =404;
		autoindex on;
	}

	listen [::]:443 ssl http2;
	listen 443 ssl http2;

	# Contains whatever TLS parameters you might want to set, such as Strict Transport Security and OCSP stapling.
	include snippets/ssl.conf;
	ssl_certificate /etc/letsencrypt/live/YOUR_SERVER_NAME/fullchain.pem;
	ssl_trusted_certificate /etc/letsencrypt/live/YOUR_SERVER_NAME/chain.pem;
	ssl_certificate_key /etc/letsencrypt/live/YOUR_SERVER_NAME/privkey.pem;
}

server {
	server_name YOUR_SERVER_NAME;

	location / {
		return 301 https://$host$request_uri;
	}

	listen 80;
	listen [::]:80;
}
```

If you have any questions or problems, feel free to open an issue.

## Contributing

PRs and issues welcome. Remember that opening issues is not annoying, it's a valuable contribution!

## License

AGPL-3.0-or-later
