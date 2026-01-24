import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/api.dart' as api;

class SettingsPage extends StatefulWidget {
  const SettingsPage({super.key});

  @override
  State<SettingsPage> createState() => _SettingsPageState();
}

class _SettingsPageState extends State<SettingsPage> {
  String? _distroboxVersion;
  bool _isLoadingVersion = true;

  @override
  void initState() {
    super.initState();
    _loadDistroboxVersion();
  }

  Future<void> _loadDistroboxVersion() async {
    setState(() => _isLoadingVersion = true);
    try {
      final version = await api.getDistroboxVersion();
      if (mounted) {
        setState(() {
          _distroboxVersion = version;
          _isLoadingVersion = false;
        });
      }
    } catch (e) {
      if (mounted) {
        setState(() {
          _distroboxVersion = 'Unknown';
          _isLoadingVersion = false;
        });
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Settings'),
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          return ListView(
            padding: const EdgeInsets.all(16),
            children: [
              // System Info Section
              const SectionHeader(title: 'System Information'),
              Card(
                elevation: 0,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: Theme.of(context).dividerColor),
                ),
                child: Column(
                  children: [
                    _buildInfoTile(
                      context,
                      icon: Icons.terminal,
                      title: 'Distrobox Version',
                      value: _isLoadingVersion
                          ? 'Loading...'
                          : _distroboxVersion ?? 'Unknown',
                      trailing: IconButton(
                        icon: const Icon(Icons.refresh, size: 20),
                        onPressed: _loadDistroboxVersion,
                      ),
                    ),
                    const Divider(height: 1),
                    _buildInfoTile(
                      context,
                      icon: Icons.inventory_2,
                      title: 'Total Containers',
                      value: '${appState.containers.length}',
                    ),
                    const Divider(height: 1),
                    _buildInfoTile(
                      context,
                      icon: Icons.play_circle,
                      title: 'Running Containers',
                      value: '${appState.runningContainersCount}',
                      valueColor: Colors.green,
                    ),
                    const Divider(height: 1),
                    _buildInfoTile(
                      context,
                      icon: Icons.check_circle,
                      title: 'Distrobox Installed',
                      value: appState.isDistroboxInstalled ? 'Yes' : 'No',
                      valueColor: appState.isDistroboxInstalled
                          ? Colors.green
                          : Colors.red,
                    ),
                  ],
                ),
              ),

              const SizedBox(height: 24),

              // Quick Actions Section
              const SectionHeader(title: 'Quick Actions'),
              Card(
                elevation: 0,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: Theme.of(context).dividerColor),
                ),
                child: Column(
                  children: [
                    _buildActionTile(
                      context,
                      icon: Icons.refresh,
                      title: 'Refresh All Data',
                      subtitle: 'Reload containers and system info',
                      onTap: () async {
                        await appState.refresh();
                        await _loadDistroboxVersion();
                        if (mounted) {
                          ScaffoldMessenger.of(context).showSnackBar(
                            const SnackBar(content: Text('Data refreshed')),
                          );
                        }
                      },
                    ),
                    const Divider(height: 1),
                    _buildActionTile(
                      context,
                      icon: Icons.stop_circle,
                      iconColor: Colors.orange,
                      title: 'Stop All Containers',
                      subtitle: 'Stop all running containers at once',
                      onTap: appState.runningContainersCount > 0
                          ? () => _confirmStopAll(context, appState)
                          : null,
                    ),
                    const Divider(height: 1),
                    _buildActionTile(
                      context,
                      icon: Icons.system_update_alt,
                      iconColor: Colors.blue,
                      title: 'Upgrade All Containers',
                      subtitle: 'Update packages in all running containers',
                      onTap: appState.runningContainersCount > 0
                          ? () {
                              ScaffoldMessenger.of(context).showSnackBar(
                                const SnackBar(
                                  content: Text(
                                      'Go to Updates page to upgrade containers'),
                                ),
                              );
                            }
                          : null,
                    ),
                    const Divider(height: 1),
                    _buildActionTile(
                      context,
                      icon: Icons.cleaning_services,
                      title: 'Clear Completed Tasks',
                      subtitle: 'Remove finished tasks from activity log',
                      onTap: () {
                        appState.clearCompletedTasks();
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(
                              content: Text('Completed tasks cleared')),
                        );
                      },
                    ),
                  ],
                ),
              ),

              const SizedBox(height: 24),

              // About Section
              const SectionHeader(title: 'About'),
              Card(
                elevation: 0,
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: Theme.of(context).dividerColor),
                ),
                child: Column(
                  children: [
                    _buildInfoTile(
                      context,
                      icon: Icons.info,
                      title: 'Gosh Distrobox Manager',
                      value: 'Distrobox Manager',
                    ),
                    const Divider(height: 1),
                    _buildActionTile(
                      context,
                      icon: Icons.code,
                      title: 'Source Code',
                      subtitle: 'View on GitHub',
                      onTap: () {
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(
                            content: Text('Open source project on GitHub'),
                          ),
                        );
                      },
                    ),
                    const Divider(height: 1),
                    _buildActionTile(
                      context,
                      icon: Icons.description,
                      title: 'Distrobox Documentation',
                      subtitle: 'Learn more about Distrobox',
                      onTap: () {
                        ScaffoldMessenger.of(context).showSnackBar(
                          const SnackBar(
                            content: Text('Visit distrobox.it for documentation'),
                          ),
                        );
                      },
                    ),
                  ],
                ),
              ),

              const SizedBox(height: 24),

              // Danger Zone
              const SectionHeader(title: 'Danger Zone', color: Colors.red),
              Card(
                elevation: 0,
                color: Colors.red.withOpacity(0.05),
                shape: RoundedRectangleBorder(
                  borderRadius: BorderRadius.circular(12),
                  side: BorderSide(color: Colors.red.withOpacity(0.2)),
                ),
                child: Column(
                  children: [
                    _buildActionTile(
                      context,
                      icon: Icons.delete_forever,
                      iconColor: Colors.red,
                      title: 'Delete All Containers',
                      subtitle: 'Remove all containers permanently',
                      titleColor: Colors.red,
                      onTap: appState.containers.isNotEmpty
                          ? () => _confirmDeleteAll(context, appState)
                          : null,
                    ),
                  ],
                ),
              ),

              const SizedBox(height: 40),
            ],
          );
        },
      ),
    );
  }

  Widget _buildInfoTile(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String value,
    Color? valueColor,
    Widget? trailing,
  }) {
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 12),
      child: Row(
        children: [
          Icon(icon, color: Theme.of(context).hintColor, size: 20),
          const SizedBox(width: 12),
          Expanded(
            child: Text(title, style: const TextStyle(fontWeight: FontWeight.w500)),
          ),
          Text(
            value,
            style: TextStyle(
              fontWeight: FontWeight.bold,
              color: valueColor ?? Theme.of(context).colorScheme.primary,
            ),
          ),
          if (trailing != null) trailing,
        ],
      ),
    );
  }

  Widget _buildActionTile(
    BuildContext context, {
    required IconData icon,
    required String title,
    required String subtitle,
    required VoidCallback? onTap,
    Color? iconColor,
    Color? titleColor,
  }) {
    final isEnabled = onTap != null;
    return ListTile(
      leading: Icon(
        icon,
        color: isEnabled
            ? (iconColor ?? Theme.of(context).hintColor)
            : Theme.of(context).disabledColor,
      ),
      title: Text(
        title,
        style: TextStyle(
          fontWeight: FontWeight.w500,
          color: isEnabled
              ? (titleColor ?? Theme.of(context).textTheme.bodyLarge?.color)
              : Theme.of(context).disabledColor,
        ),
      ),
      subtitle: Text(
        subtitle,
        style: TextStyle(
          fontSize: 12,
          color: isEnabled
              ? Theme.of(context).hintColor
              : Theme.of(context).disabledColor,
        ),
      ),
      trailing: Icon(
        Icons.chevron_right,
        color:
            isEnabled ? Theme.of(context).hintColor : Theme.of(context).disabledColor,
      ),
      onTap: onTap,
    );
  }

  void _confirmStopAll(BuildContext context, AppStateProvider appState) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Stop All Containers'),
        content: Text(
          'This will stop all ${appState.runningContainersCount} running containers. Continue?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              Navigator.pop(context);
              await appState.stopAllContainers();
              if (context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('All containers stopped')),
                );
              }
            },
            style: FilledButton.styleFrom(backgroundColor: Colors.orange),
            child: const Text('Stop All'),
          ),
        ],
      ),
    );
  }

  void _confirmDeleteAll(BuildContext context, AppStateProvider appState) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete All Containers'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'This will permanently delete all ${appState.containers.length} containers!',
            ),
            const SizedBox(height: 16),
            Container(
              padding: const EdgeInsets.all(12),
              decoration: BoxDecoration(
                color: Colors.red.withOpacity(0.1),
                borderRadius: BorderRadius.circular(8),
              ),
              child: const Row(
                children: [
                  Icon(Icons.warning, color: Colors.red),
                  SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      'This action cannot be undone. All container data will be lost.',
                      style: TextStyle(color: Colors.red, fontSize: 12),
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () async {
              Navigator.pop(context);
              // Delete each container
              for (final container in appState.containers.toList()) {
                await appState.removeContainer(container.name);
              }
              if (context.mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('All containers deleted')),
                );
              }
            },
            style: FilledButton.styleFrom(backgroundColor: Colors.red),
            child: const Text('Delete All'),
          ),
        ],
      ),
    );
  }
}

class SectionHeader extends StatelessWidget {
  final String title;
  final Color? color;

  const SectionHeader({super.key, required this.title, this.color});

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 8, left: 4),
      child: Text(
        title.toUpperCase(),
        style: TextStyle(
          fontSize: 12,
          fontWeight: FontWeight.bold,
          color: color ?? Colors.grey,
          letterSpacing: 1.0,
        ),
      ),
    );
  }
}
