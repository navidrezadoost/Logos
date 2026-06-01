---
title: 1.3  Install with Docker
desc: This Logos technical guide covers self-hosting, Docker installation, configuration, updates, backups, and proxy setup with NGINX and Caddy. Try Logos!
---

<p class="advice">
Installing and maintaining a self-hosted Logos instance requires some technical knowledge:
Docker and Docker Compose, basic DNS management, and proxy configuration.
If you're not comfortable with this stack, we encourage you to try
more straight-forward installations with <a href="https://help.logos.app/technical-guide/getting-started/elestio/" target="_blank">Elestio</a>
or use the SAAS at <a href="https://design.logos.app" targret="_blank">https://design.logos.app</a>.
</p>

# Install with Docker

This section details everything you need to know to get Logos up and running in
production environments using Docker. For this, we provide a series of *Dockerfiles* and a
*docker-compose* file that orchestrate all.

## Install Docker

To host a Logos instance with Docker, it's necessary to have
<code class="language-bash">docker</code> and <code class="language-bash">docker compose</code>
installed. Check the comprehensive <a href="https://docs.docker.com/" target="_blank">official documentation</a>
to install and maintain docker.

## Start Logos

> **Coming soon:** The official `docker-compose.yaml` and pre-built Docker images for the
> community edition are being prepared and will be published under the `logos/` Docker Hub
> namespace at the first stable release.
>
> Once available, you will be able to start a self-hosted instance with:
>
> ```bash
> wget https://raw.githubusercontent.com/logos/logos/main/docker-compose.yaml
> docker compose -p logos -f docker-compose.yaml up -d
> ```
>
> Until then, you can build and run the Go backend directly from source — see the
> [Developer guide](../developer/) for details.

At the end it will start listening on http://localhost:9001

<p class="advice">
    If you don't change anything, by default this will use the latest image published in dockerhub.
</p>

If you want to have more control over the version (which is recommended), you can use the LOGOS_VERSION envvar in the common ways:
- setting the value in the .env file
- or passing the envvar in the command line

```bash
LOGOS_VERSION=2.4.3 docker compose -p logos -f docker-compose.yaml up -d
```

## Stop Logos

If you want to stop running Logos, just type

```bash
docker compose -p logos -f docker-compose.yaml down
```

## Configure Logos with Docker

The configuration is defined using flags and environment variables in the <code class="language-bash">docker-compose.yaml</code>
file. The default downloaded file comes with the essential flags and variables already set,
and other ones commented out with some explanations.

You can find all configuration options in the [Configuration][1] section.

## Using the CLI for administrative tasks

Logos provides a script (`manage.py`) with some administrative tasks to perform in the server.

**NOTE**: this script will only work with the <code class="language-bash">enable-prepl-server</code>
flag set in the docker-compose.yaml file. For older versions of docker-compose.yaml file,
this flag is set in the backend service.

For instance, if  the registration is disabled, the only way to create a new user is with this script:

```bash
docker exec -ti logos-logos-backend-1 python3 manage.py create-profile
```

or

```bash
docker exec -ti logos-logos-backend-1 python3 manage.py create-profile --skip-tutorial --skip-walkthrough
```


**NOTE:** the exact container name depends on your docker version and platform.
For example it could be <code class="language-bash">logos-logos-backend-1</code> or <code class="language-bash">logos_logos-backend-1</code>.
You can check the correct name executing <code class="language-bash">docker ps</code>.

## Update Logos

To get the latest version of Logos in your local installation, you just need to
execute:

```bash
docker compose -f docker-compose.yaml pull
```

This will fetch the latest images. When you do <code class="language-bash">docker compose up</code> again, the containers will be recreated with the latest version.

<p class="advice">
    It is strongly recommended to update the Logos version in small increments, rather than updating between two distant versions.
</p>

**Important: Upgrade from version 1.x to 2.0**

The migration to version 2.0, due to the incorporation of the new v2 components, includes
an additional process that runs automatically as soon as the application starts. If your
on-premises Logos instance contains a significant amount of data (such as hundreds of
logos files, especially those utilizing SVG components and assets extensively), this
process may take a few minutes.

In some cases, such as when the script encounters an error, it may be convenient to run
the process manually. To do this, you can disable the automatic migration process using
the <code class="language-bash">disable-v2-migration</code> flag in <code
class="language-bash">LOGOS_FLAGS</code> environment variable. You can then execute the
migration process manually with the following command:

```bash
docker exec -ti <container-name-or-id> ./run.sh app.migrations.v2
```

**IMPORTANT:** this script should be executed on passing from 1.19.x to 2.0.x. Executing
it on versions greater or equal to 2.1 of logos will not work correctly. It is known that
this script is removed since 2.4.3


## Backup Logos

Logos uses <a href="https://docs.docker.com/storage/volumes" target="_blank">Docker
volumes</a> to store all persistent data. This allows you to delete and recreate
containers whenever you want without losing information.

This also means you need to do regular backups of the contents of the volumes. You cannot
directly copy the contents of the volume data folder. Docker provides you a <a
href="https://docs.docker.com/storage/volumes/#back-up-restore-or-migrate-data-volumes"
target="_blank">volume backup procedure</a>, that uses a temporary container to mount one
or more volumes, and copy their data to an archive file stored outside of the container.

If you use Docker Desktop, <a
href="https://www.docker.com/blog/back-up-and-share-docker-volumes-with-this-extension/"
target="_blank">there is an extension</a> that may ease the backup process.

If you use the default **docker compose** file, there are two volumes used: one for the
Postgres database and another one for the assets uploaded by your users (images and svg
clips). There may be more volumes if you enable other features, as explained in the file
itself.

## Configure the proxy and HTTPS

We strongly recommend to use Logos under HTTPS/SSL, which will require specific server configurations for DNS and SSL certificates.
Besides, your host configuration needs to make a proxy to http://localhost:9001.

<p class="advice">
    If you plan to serve Logos under different domain than `localhost` without HTTPS,
    you need to disable the `secure` flag on cookies, with the `disable-secure-session-cookies` flag.
    This is a configuration NOT recommended for production environments; as some browser APIs do
    not work properly under non-https environments, this unsecure configuration
    may limit the usage of Logos; as an example, the clipboard does not work with HTTP.
</p>

Below, you can see three examples with three different proxys:

### Example with NGINX

```bash
server {
  listen 80;
  server_name logos.mycompany.com;
  return 301 https://$host$request_uri;
}

server {
  listen 443 ssl;
  server_name logos.mycompany.com;

  # This value should be in sync with the corresponding in the docker-compose.yml
  # LOGOS_HTTP_SERVER_MAX_BODY_SIZE: 31457280
  client_max_body_size 31457280;

  # Logs: Configure your logs following the best practices inside your company
  access_log /path/to/logos.access.log;
  error_log /path/to/logos.error.log;

  # TLS: Configure your TLS following the best practices inside your company
  ssl_certificate /path/to/fullchain;
  ssl_certificate_key /path/to/privkey;

  # Websockets
  location /ws/notifications {
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection 'upgrade';
    proxy_pass http://localhost:9001/ws/notifications;
  }

  # Proxy pass
  location / {
    proxy_set_header Host $http_host;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Scheme $scheme;
    proxy_set_header X-Forwarded-Proto $scheme;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_redirect off;
    proxy_pass http://localhost:9001/;
  }
}
```

For full documentation, go to the [official website][2]

### Example with CADDY SERVER

```bash
logos.mycompany.com {
        reverse_proxy :9001
        tls /path/to/fullchain.pem /path/to/privkey.pem
        log {
            output file /path/to/logos.log
        }
}
```

For full documentation, go to the [official website][3]

### Example with TRAEFIK

In the [Logos's docker-compose.yaml][4] file, there is a simple example with Traefik.
For full documentation, go to the [official website][5]

[1]: /technical-guide/configuration/
[2]: https://nginx.org/en/docs/index.html
[3]: https://caddyserver.com/docs/
[5]: https://doc.traefik.io/traefik/
