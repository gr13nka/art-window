# Hosting the Art Window waitlist

**Address:** `https://artwindow.alps-project.online`

The site is a static page plus one small PHP script. There is no build step, no
database and no dependency to install.

| File | What it is |
|---|---|
| `index.html`, `style.css`, `img/` | the landing page, including the iPhone & iPad waitlist form |
| `subscribe.php` | receives the form and appends the address to a CSV |

**A static host cannot run this.** GitHub Pages (or any static host) serves
`index.html` fine but cannot execute PHP, so the waitlist form needs a PHP host.
On a static host the form will show "Didn’t go through".

## Requirements

- A web server that runs PHP **7.4 or newer** (Apache with mod_php, or nginx with php-fpm).
- **HTTPS** on the domain (Let's Encrypt is fine). The form must not post over plain http.
- A directory **outside the web root** that the PHP user can write to.

## Steps

1. **DNS.** Point `artwindow.alps-project.online` at the server (A/AAAA or CNAME) and issue an HTTPS
   certificate for it.

2. **Upload the site** into the web root, side by side:
   ```
   /var/www/art-window/            ← web root
     index.html
     style.css
     img/
     subscribe.php
   ```
   The page posts to `subscribe.php` relative to itself. Keep them together.

3. **Create the data directory outside the web root.** It must be writable by the PHP user
   (commonly `www-data`):
   ```sh
   sudo mkdir -p /var/www/art-window-data
   sudo chown www-data:www-data /var/www/art-window-data
   sudo chmod 700 /var/www/art-window-data
   ```
   By default `subscribe.php` writes to `../art-window-data/waitlist.csv`, relative to
   itself. That resolves to the folder above if the layout matches. To put the file
   anywhere else, set an environment variable for PHP:
   - php-fpm pool: `env[ARTWINDOW_WAITLIST_FILE] = /srv/data/art-window/waitlist.csv`
   - Apache: `SetEnv ARTWINDOW_WAITLIST_FILE /srv/data/art-window/waitlist.csv`

   **Never put the CSV inside the web root.** Anyone could download it.

4. **Server config.**

   nginx. First, in the `http { … }` block — the `limit_req` below names this zone,
   and `nginx -t` fails without it:
   ```nginx
   # at most a few signups per minute from one address
   limit_req_zone $binary_remote_addr zone=artwindow:1m rate=6r/m;
   ```
   Then inside the site's `server { … }`, with the certificate lines already there:
   ```nginx
   root /var/www/art-window;
   index index.html;

   location = /subscribe.php {
       limit_req zone=artwindow burst=5 nodelay;
       include snippets/fastcgi-php.conf;
       fastcgi_pass unix:/run/php/php8.2-fpm.sock;   # match the installed PHP version
   }
   location ~ \.php$ { return 404; }   # no other PHP is served
   location ~ /\.   { deny all; }
   ```
   Apache: the defaults work as they are. `mod_evasive` or a WAF rule can do the rate limiting.

5. **Test it** from any machine:
   ```sh
   curl -i -X POST -F email=test@example.com https://artwindow.alps-project.online/subscribe.php
   # → HTTP 200 {"ok":true}
   curl -i -X POST -F email=nope https://artwindow.alps-project.online/subscribe.php
   # → HTTP 422 {"ok":false,"error":"email"}
   curl -i https://artwindow.alps-project.online/subscribe.php
   # → HTTP 405
   ```
   Then check that `waitlist.csv` exists and has the test line. **Delete the test line**
   afterwards.

   Finally, open the page in a browser and submit a real address. On the live domain a
   success means the address really was stored. Only a local copy of the page (a
   `file:` URL, localhost, or `?demo`) plays the success message without sending anything.

## The data

`waitlist.csv` looks like this:
```
email,signed_up_utc
someone@example.com,2026-09-28T14:03:11Z
```
It holds only the address and the date, which is exactly what the form says it keeps.
Keep it that way:

- **Don't add analytics, cookies, tracking pixels or third-party scripts** to the
  page. Don't move the list to Mailchimp, Google Sheets or any other outside service.
- **Access:** only the team. Keep the directory `chmod 700`.
- **Backups:** include `art-window-data/` in the server's normal encrypted backups.
- **Deletion requests:** delete the person's line when they ask.
- **Unsubscribes:** when launch emails go out, remove anyone who unsubscribes from
  the CSV.
- Standard web-server access logs are fine for security and stable operation.
- **The privacy policy promises all of this.** It is the `#privacy` sheet at the
  bottom of `index.html`. Change how the list is handled and you change the policy
  with it, including its "last updated" date.
