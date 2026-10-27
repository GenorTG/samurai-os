# ADR-0006 — firewalld as the default firewall

**Status:** proposed

## Decision
firewalld, enabled, default zone `public` (inbound denied). ufw and OpenSnitch remain toolbox
options (Garuda parity); OpenSnitch can run alongside firewalld because it filters outbound traffic
per application.

## Why
- Zones fit laptops that move between home, office and public Wi-Fi; NetworkManager can assign a zone
  per connection.
- Plasma's firewall settings module supports firewalld, and libvirt and Podman integrate with it.
- ufw is simpler but has no zone concept and conflicts with Docker's rules in confusing ways (so does
  firewalld, less so). Docker's `iptables` behaviour is documented in the security page either way.
