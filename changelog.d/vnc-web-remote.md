Added:

- The web remote has a second page, VNC (`/vnc`): the screen through noVNC, which sends only what changed on it
  instead of a whole picture each time, and takes the mouse as a finger, held and dragged as on the scope. The
  wheel, the right button and the keys act as those of a mouse and a keyboard on the scope. Two viewers at most
  ([HTTP reference](docs/http.md)).
- Remote services has a new switch, VNC, off by default: port 5900 for a desktop VNC viewer, with the same screen,
  touch, keys and wheel as the VNC page; it gets the screen compressed (Tight) where the viewer takes that. In
  Minimal and Studio the Remote services sheet has a third tab, Addresses, and mDNS moved to Access
  ([Network](docs/network.md)).
