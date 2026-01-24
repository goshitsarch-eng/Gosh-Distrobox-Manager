import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/src/rust/api.dart' as api;
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';

class AppsPage extends StatefulWidget {
  final ContainerInfo container;

  const AppsPage({super.key, required this.container});

  @override
  State<AppsPage> createState() => _AppsPageState();
}

class _AppsPageState extends State<AppsPage> {
  final TextEditingController _searchController = TextEditingController();
  final TextEditingController _binaryPathController = TextEditingController();
  String _searchQuery = '';

  @override
  void initState() {
    super.initState();
    // Load apps when page opens
    WidgetsBinding.instance.addPostFrameCallback((_) {
      context.read<AppStateProvider>().loadContainerApps(widget.container.name);
    });
  }

  @override
  void dispose() {
    _searchController.dispose();
    _binaryPathController.dispose();
    super.dispose();
  }

  List<api.AppInfo> _filterApps(List<api.AppInfo> apps) {
    if (_searchQuery.isEmpty) return apps;
    final query = _searchQuery.toLowerCase();
    return apps.where((app) {
      return app.name.toLowerCase().contains(query) ||
          app.exec.toLowerCase().contains(query);
    }).toList();
  }

  void _showBinaryExportDialog() {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Export Binary'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              'Enter the path to a binary inside the container to export it to your host system.',
              style: TextStyle(color: Theme.of(context).hintColor),
            ),
            const SizedBox(height: 16),
            TextField(
              controller: _binaryPathController,
              decoration: const InputDecoration(
                labelText: 'Binary Path',
                hintText: '/usr/bin/some-command',
                border: OutlineInputBorder(),
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
              if (_binaryPathController.text.isEmpty) return;
              Navigator.pop(context);
              final appState = context.read<AppStateProvider>();
              final success = await appState.exportBinary(
                widget.container.name,
                _binaryPathController.text,
              );
              if (success && mounted) {
                ScaffoldMessenger.of(context).showSnackBar(
                  const SnackBar(content: Text('Binary exported successfully')),
                );
              }
              _binaryPathController.clear();
            },
            child: const Text('Export'),
          ),
        ],
      ),
    );
  }

  IconData _getAppIcon(String iconName) {
    // Try to match common app names to icons
    final nameLower = iconName.toLowerCase();
    if (nameLower.contains('firefox') || nameLower.contains('browser')) {
      return Icons.public;
    }
    if (nameLower.contains('code') || nameLower.contains('editor')) {
      return Icons.code;
    }
    if (nameLower.contains('terminal') || nameLower.contains('console')) {
      return Icons.terminal;
    }
    if (nameLower.contains('file') || nameLower.contains('folder')) {
      return Icons.folder;
    }
    if (nameLower.contains('music') || nameLower.contains('audio')) {
      return Icons.music_note;
    }
    if (nameLower.contains('video') || nameLower.contains('vlc')) {
      return Icons.video_library;
    }
    if (nameLower.contains('image') || nameLower.contains('gimp') || nameLower.contains('photo')) {
      return Icons.image;
    }
    if (nameLower.contains('mail') || nameLower.contains('email')) {
      return Icons.email;
    }
    if (nameLower.contains('chat') || nameLower.contains('message') || nameLower.contains('telegram')) {
      return Icons.chat;
    }
    if (nameLower.contains('settings') || nameLower.contains('config')) {
      return Icons.settings;
    }
    return Icons.apps;
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            const Text('App Manager',
                style: TextStyle(fontSize: 16, fontWeight: FontWeight.bold)),
            Text('Container: ${widget.container.name}',
                style: TextStyle(
                    fontSize: 12,
                    color: Theme.of(context).textTheme.bodySmall?.color)),
          ],
        ),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () {
              context
                  .read<AppStateProvider>()
                  .loadContainerApps(widget.container.name);
            },
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoadingApps) {
            return const Center(child: CircularProgressIndicator());
          }

          if (appState.appsError != null) {
            return Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Icon(Icons.error_outline,
                      size: 48, color: Theme.of(context).colorScheme.error),
                  const SizedBox(height: 16),
                  Text(appState.appsError!,
                      textAlign: TextAlign.center,
                      style:
                          TextStyle(color: Theme.of(context).colorScheme.error)),
                  const SizedBox(height: 16),
                  FilledButton.icon(
                    onPressed: () => appState.loadContainerApps(widget.container.name),
                    icon: const Icon(Icons.refresh),
                    label: const Text('Retry'),
                  ),
                ],
              ),
            );
          }

          final filteredApps = _filterApps(appState.containerApps);

          return Column(
            children: [
              // Search Bar
              Padding(
                padding: const EdgeInsets.all(16.0),
                child: SearchBar(
                  controller: _searchController,
                  leading: const Icon(Icons.search),
                  hintText: 'Search applications...',
                  onChanged: (value) {
                    setState(() => _searchQuery = value);
                  },
                  elevation: WidgetStateProperty.all(0),
                  backgroundColor: WidgetStateProperty.resolveWith((states) {
                    return Theme.of(context).brightness == Brightness.dark
                        ? Colors.grey[800]
                        : Colors.white;
                  }),
                  shape: WidgetStateProperty.all(RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(12),
                    side: BorderSide(color: Theme.of(context).dividerColor),
                  )),
                ),
              ),

              // Action Button
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 16.0),
                child: FilledButton.icon(
                  onPressed: _showBinaryExportDialog,
                  icon: const Icon(Icons.add_box),
                  label: const Text('Manual Binary Export'),
                  style: FilledButton.styleFrom(
                    minimumSize: const Size(double.infinity, 48),
                    shape: RoundedRectangleBorder(
                        borderRadius: BorderRadius.circular(12)),
                  ),
                ),
              ),

              // Section Header
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
                child: Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    const Text('Installed in Container',
                        style: TextStyle(
                            fontSize: 16, fontWeight: FontWeight.bold)),
                    Container(
                      padding: const EdgeInsets.symmetric(
                          horizontal: 8, vertical: 4),
                      decoration: BoxDecoration(
                        color: Theme.of(context)
                            .colorScheme
                            .primary
                            .withOpacity(0.1),
                        borderRadius: BorderRadius.circular(12),
                      ),
                      child: Text('${filteredApps.length} Found',
                          style: TextStyle(
                              color: Theme.of(context).colorScheme.primary,
                              fontWeight: FontWeight.bold,
                              fontSize: 12)),
                    ),
                  ],
                ),
              ),

              // App Grid or Empty State
              Expanded(
                child: filteredApps.isEmpty
                    ? Center(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Icon(Icons.apps_outage,
                                size: 64, color: Theme.of(context).hintColor),
                            const SizedBox(height: 16),
                            Text(
                              _searchQuery.isEmpty
                                  ? 'No applications found in this container'
                                  : 'No applications match your search',
                              style:
                                  TextStyle(color: Theme.of(context).hintColor),
                            ),
                          ],
                        ),
                      )
                    : GridView.builder(
                        gridDelegate:
                            const SliverGridDelegateWithFixedCrossAxisCount(
                          crossAxisCount: 2,
                          mainAxisSpacing: 16,
                          crossAxisSpacing: 16,
                          childAspectRatio: 1.1,
                        ),
                        padding: const EdgeInsets.all(16),
                        itemCount: filteredApps.length,
                        itemBuilder: (context, index) {
                          final app = filteredApps[index];
                          return _buildAppCard(context, app, appState);
                        },
                      ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _buildAppCard(
      BuildContext context, api.AppInfo app, AppStateProvider appState) {
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: Theme.of(context).cardColor,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(
            color: Theme.of(context).dividerColor.withOpacity(0.5)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 48,
                height: 48,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(12),
                  color: Theme.of(context)
                      .colorScheme
                      .primary
                      .withOpacity(0.1),
                ),
                child: Icon(
                  _getAppIcon(app.icon.isNotEmpty ? app.icon : app.name),
                  color: Theme.of(context).colorScheme.primary,
                ),
              ),
              Switch(
                value: app.isExported,
                onChanged: appState.isActionInProgress
                    ? null
                    : (value) async {
                        await appState.toggleAppExport(
                          widget.container.name,
                          app,
                          value,
                        );
                      },
                activeColor: Theme.of(context).colorScheme.primary,
              ),
            ],
          ),
          const Spacer(),
          Text(app.name,
              style:
                  const TextStyle(fontWeight: FontWeight.bold, fontSize: 14),
              maxLines: 1,
              overflow: TextOverflow.ellipsis),
          Text(
            app.exec,
            style: TextStyle(fontSize: 12, color: Theme.of(context).hintColor),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
          const SizedBox(height: 4),
          Text(
            app.isExported ? 'EXPORTED' : 'NOT EXPORTED',
            style: TextStyle(
              fontSize: 10,
              fontWeight: FontWeight.bold,
              color: app.isExported ? Colors.green : Colors.grey,
              letterSpacing: 0.5,
            ),
          ),
        ],
      ),
    );
  }
}
