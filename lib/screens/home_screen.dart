import 'package:flutter/material.dart';
import 'package:gosh_distrobox_manager/screens/containers_page.dart';
import 'package:gosh_distrobox_manager/screens/dashboard_page.dart';
import 'package:gosh_distrobox_manager/screens/images_page.dart';
import 'package:gosh_distrobox_manager/screens/package_manager_page.dart';
import 'package:gosh_distrobox_manager/screens/backups_page.dart';
import 'package:gosh_distrobox_manager/screens/activity_logs_page.dart';
import 'package:gosh_distrobox_manager/screens/settings_page.dart';
import 'package:gosh_distrobox_manager/screens/updates_page.dart';

class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  int _selectedIndex = 0;

  final List<Widget> _pages = [
    const DashboardPage(),
    const ContainersPage(),
    const ImagesPage(),
    const PackageManagerPage(),
    const UpdatesPage(),
    const BackupsPage(),
    const ActivityLogsPage(),
    const SettingsPage(),
  ];

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth < 600) {
          // Mobile View: Bottom Navigation Bar
          return Scaffold(
            body: _pages[_selectedIndex],
            bottomNavigationBar: NavigationBar(
              selectedIndex: _selectedIndex,
              onDestinationSelected: (index) {
                setState(() {
                  _selectedIndex = index;
                });
              },
              destinations: const [
                NavigationDestination(icon: Icon(Icons.dashboard_outlined), selectedIcon: Icon(Icons.dashboard), label: 'Home'),
                NavigationDestination(icon: Icon(Icons.inventory_2_outlined), selectedIcon: Icon(Icons.inventory_2), label: 'Boxes'),
                NavigationDestination(icon: Icon(Icons.album_outlined), selectedIcon: Icon(Icons.album), label: 'Images'),
                NavigationDestination(icon: Icon(Icons.inventory_2_outlined), selectedIcon: Icon(Icons.inventory_2), label: 'Pkgs'),
                NavigationDestination(icon: Icon(Icons.system_update_alt_outlined), selectedIcon: Icon(Icons.system_update_alt), label: 'Updates'),
                NavigationDestination(icon: Icon(Icons.backup_outlined), selectedIcon: Icon(Icons.backup), label: 'Backups'),
                NavigationDestination(icon: Icon(Icons.history_toggle_off_outlined), selectedIcon: Icon(Icons.history_toggle_off), label: 'Logs'),
                NavigationDestination(icon: Icon(Icons.settings_outlined), selectedIcon: Icon(Icons.settings), label: 'Settings'),
              ],
            ),
          );
        } else {
          // Desktop View: Navigation Rail
          return Scaffold(
            body: Row(
              children: [
                NavigationRail(
                  extended: constraints.maxWidth >= 1000, // Expand rail on wide screens
                  selectedIndex: _selectedIndex,
                  onDestinationSelected: (index) {
                    setState(() {
                      _selectedIndex = index;
                    });
                  },
                  labelType: constraints.maxWidth >= 1000 
                      ? NavigationRailLabelType.none 
                      : NavigationRailLabelType.all,
                  destinations: const [
                    NavigationRailDestination(icon: Icon(Icons.dashboard_outlined), selectedIcon: Icon(Icons.dashboard), label: Text('Home')),
                    NavigationRailDestination(icon: Icon(Icons.inventory_2_outlined), selectedIcon: Icon(Icons.inventory_2), label: Text('Containers')),
                    NavigationRailDestination(icon: Icon(Icons.album_outlined), selectedIcon: Icon(Icons.album), label: Text('Images')),
                    NavigationRailDestination(icon: Icon(Icons.inventory_2_outlined), selectedIcon: Icon(Icons.inventory_2), label: Text('Packages')),
                    NavigationRailDestination(icon: Icon(Icons.system_update_alt_outlined), selectedIcon: Icon(Icons.system_update_alt), label: Text('Updates')),
                    NavigationRailDestination(icon: Icon(Icons.backup_outlined), selectedIcon: Icon(Icons.backup), label: Text('Backups')),
                    NavigationRailDestination(icon: Icon(Icons.history_toggle_off_outlined), selectedIcon: Icon(Icons.history_toggle_off), label: Text('Logs')),
                    NavigationRailDestination(icon: Icon(Icons.settings_outlined), selectedIcon: Icon(Icons.settings), label: Text('Settings')),
                  ],
                ),
                const VerticalDivider(thickness: 1, width: 1),
                Expanded(child: _pages[_selectedIndex]),
              ],
            ),
          );
        }
      },
    );
  }
}
