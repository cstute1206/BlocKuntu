## Test Idea

Tests will run locally, with the Virtual machine manager. For testing we will clone the VMs with the name "CachyOS", "Ubuntu" and "Fedora".  We will rename them accordingly. Then we will test on each of these VMs the tests. They will do the following:

### Tests

1. Install BlocKuntu
2. Install Chrome extension and Firefox extension in the following browsers (or should we already have this extension installed?). Also gotta see, what browsers are available on which Distro:
  1. LibreWolf
  2. WaterFox
  3. Fireforx deb
  4. Firefox Flatpak
  5. Firefox Snap
  6. Chromium deb
  7. Chrome rpm
  8. Chrome deb
  9. Chromium Snap
  10. Edge
  11. Opera
  12. Vivaldi
  13. Brave
3. Append a blockuntu-policy.toml
5. Open Browsers and close them
6. Check enforcment of browser policy
7. Check if website of blockuntu-policy.toml is blocked. 
  1. Check by Domain
  2. Check by  Exact URL
  3. Check by URL Prefix
  4. Check by  URL containers
  5. Check by Path Prefix
7. Check daily allowance. Does it work on a website?
8. Check daily allowance. Does it work for an application block
9. Check detox, does it block websites and applications? Does it also unblock, when the detox is finished?
10. Doese a schedule work? Does it block and unblock an application and website?
11. Check application block
12. Does adding to website list work, while it is active?
13. Does adding to application list work, while it is active?
14. What happens, when we have two active rules? Is the stricter one enforced?
15. Does Tier 1, Tier 2 and Tier 3 work?
16. Does uninstall work with the key?
17. Does edit work with the key?
18. Does export of blockuntu-policy.toml work?
19. Does update work?
20. Is the hardening still active?
  1. Can I just uninstall over the cli?
  2. Can I modify the hosts file? 
  3. Can I delete the hosts file?
  4. Can I delete the .sqlite database?
  5. What happens, when the user fiddles with the time settings?
21. Does "protected setting access" work, when we change it?
22. Can we watch a youtube video for 1 hour withou losing the heartbeat?
