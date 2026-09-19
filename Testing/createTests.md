## Test Idea

Tests will run locally, with the Virtual machine manager. For testing we will clone the VMs with the name "CachyOS", "Ubuntu" and "Fedora".  We will rename them accordingly. Then we will test on each of these VMs the tests. Also use Playwright for the Webtesting? They will do the following:

### ToDo

- Generate relevant blockuntu-policy.toml

### Tests

1. Install BlocKuntu with "sudo apt install ./packagename" and then elevated priviliges for Blockuntu. .deb on Ubuntu VM, .rpm on Fedora VM and .tar.gz on CachyOS VM.
  - Target: BlocKuntu is installed without errors. You can open the GUI and the Dameon is online.
2. Install Chrome extension and Firefox extension in the following browsers (or should we already have this extension installed?). Also gotta see, what browsers are available on which Distro:
  - LibreWolf
  - WaterFox
  - Fireforx deb
  - Firefox Flatpak
  - Firefox Snap
  - Chromium deb
  - Chrome rpm
  - Chrome deb
  - Chromium Snap
  - Edge
  - Opera
  - Vivaldi
  - Brave
  - Target: You can open a website and it doesn't get interrupted because of the missed heartbeat
3. Append a blockuntu-policy.toml
  - Target: We now have the rules of the blockuntu-policy in our GUI
4. Open Browsers and close them to check enforcment of browser policiy
  - Target: We now have the browser policies enforced. See if we can't deactivate the browser extension anymore and if it is also active in incognito mode
5. Check if website of blockuntu-policy.toml is blocked.
  - Check by Domain
  - Check by  Exact URL
  - Check by URL Prefix
  - Check by  URL containers
  - Check by Path Prefix
  - Target: Websites that are in these filters are blocked
6. Check daily allowance. Does it work on a website?
  - Target:  We visit a Website for 1 Minute and then we can't visit this site anymore.
7. Check daily allowance. Does it work for an application block
  - Target: We open an Application and it gets automatically closed after 1 Minute
8. Check detox, does it block websites and applications? Does it also unblock, when the detox is finished?
  - Target: Detox is not active, we can acess the website. Detox is active we can't access the website. Detox is finished, we can acess the websites.
9. Does a schedule work? Does it block and unblock an application and website?
  - Target: Schedule is not active, we can acess the website. Schedule is not active anymore, we can't access the website. Detox is finished, we can access the website.
10. Check application block
  - Target: An application is blocked, when we try to open it and althoug it was open
11. Does adding to website list work, while it is active?
  - Target: The website list is marked is active, we then add a new domain to this website list, while it active. Then see if this new website get blocked.
12. Does adding to application list work, while it is active?
  - Target: An application list is active, we then add a new application to this, while its active, then see if this new list gets blocked.
13. What happens, when we have two active rules? Is the stricter one enforced?
  - Add two rules for a website (one with daily limit and one without). We shouldn't be able to access it, because there is no daily limit.
14. Does Tier 1, Tier 2 and Tier 3 work?
  - Target: Tier 1 is always blocked. We can't access it although it isn't even attached to any schedule. Tier 2 is blocked, when there is a schedule, but we also can't access it with the manual unlock. It should be also added in the hosts file. Tier 3 is also only active, when schedule or detox, but there is a manual unlock and it shouldn't be in the Hosts file.
15. Does uninstall work with the key?
  - Target: successfull uninstall with the key, that appears in the welcome modal
16. Does edit work with the key?
  - Target: successfull edit a Tier 1 list, after using the key and then not being able to further edit after 5 minutes
17. Does export of blockuntu-policy.toml work?
  - Target: We are able to export the settings and can save them somewhere
18. Does update work?
  - Target: We already have BlocKuntu installed and use update to upgrade the package
19. Is the hardening still active?
  1. Can I just uninstall over the cli?
    - We can't uninstall with the cli sudo apt uninstall does fail.
  2. Can I modify the hosts file?
    - Do chatter -i and then delete the hosts things. Will the hosts file be restored?
  3. Can I delete the hosts file?
    - Target: When I delete it, it will be restored.
  4. Can I delete the .sqlite database?
    - Target: When I delete the .sqlite database it  will be restored
  5. What happens, when the user fiddles with the time settings?
    - Target: The Rules are still active
20. Does "protected setting access" work, when we change it?
  - Target: We can't uninstall it anymore, when a schedule or detox is active
21. Can we watch a youtube video for 1 hour withou losing the heartbeat?
  - Target: We get no missed heartbeat error.
