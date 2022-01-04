#!/bin/sh

# Here, I use the username `icelk` and have `server` configured the target IP in `/etc/hosts`

ssh server "mkdir -p ~/kvarn/kvarn-reference/mail/public"

# Renew and change permissions
ssh root@server "certbot renew && chown icelk:icelk -R /etc/letsencrypt && chmod o-r,g-r -R /etc/letsencrypt && cp /etc/letsencrypt/live/icelk.dev/fullchain.pem /etc/unbound/cert.pem && cp /etc/letsencrypt/live/icelk.dev/privkey.pem /etc/unbound/pk.pem"
# Pull to local
rsync -rPhL --del icelk@server:/etc/letsencrypt/live/ ~/.private/certs/
# change permissions on local
chmod -R g-r,o-r ~/.private/certs
# sync with remote
rsync -rLPh --exclude target --exclude .git . server:/home/icelk/kvarn/kvarn-reference/
# Restart kvarn
ssh root@server systemctl restart kvarn
