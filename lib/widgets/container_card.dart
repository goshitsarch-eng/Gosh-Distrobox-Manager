import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/src/rust/backends/distrobox/distrobox.dart';
import 'package:gosh_distrobox_manager/screens/container_details_page.dart';
import 'package:gosh_distrobox_manager/screens/container_terminal_page.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';

class ContainerCard extends StatelessWidget {
  final ContainerInfo container;

  const ContainerCard({super.key, required this.container});

  Color _getStatusColor(Status status) {
    return switch (status) {
      Status_Up() => Colors.green,
      Status_Created() => Colors.blue,
      Status_Exited() => Colors.orange,
      Status_Other() => Colors.grey,
    };
  }

  String _getStatusText(Status status) {
    return switch (status) {
      Status_Up(field0: final s) => 'Running${s.isNotEmpty ? ' $s' : ''}',
      Status_Created(field0: final s) => 'Created${s.isNotEmpty ? ' $s' : ''}',
      Status_Exited(field0: final s) => 'Exited${s.isNotEmpty ? ' $s' : ''}',
      Status_Other(field0: final s) => s.isNotEmpty ? s : 'Unknown',
    };
  }

  IconData _getDistroIcon(String image) {
    final imageLower = image.toLowerCase();
    if (imageLower.contains('ubuntu')) return Icons.circle;
    if (imageLower.contains('fedora')) return Icons.filter_vintage;
    if (imageLower.contains('arch')) return Icons.architecture;
    if (imageLower.contains('debian')) return Icons.donut_large;
    if (imageLower.contains('alpine')) return Icons.landscape;
    if (imageLower.contains('centos') || imageLower.contains('rocky')) {
      return Icons.shield;
    }
    if (imageLower.contains('opensuse') || imageLower.contains('suse')) {
      return Icons.pets;
    }
    return Icons.dns;
  }

  void _showQuickActionsMenu(BuildContext context) {
    final appState = context.read<AppStateProvider>();
    final isRunning = container.status is Status_Up;

    showModalBottomSheet(
      context: context,
      builder: (context) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.info_outline),
              title: const Text('Details'),
              onTap: () {
                Navigator.pop(context);
                _openDetails(context);
              },
            ),
            ListTile(
              leading: const Icon(Icons.terminal),
              title: const Text('Open Terminal'),
              enabled: isRunning,
              onTap: isRunning
                  ? () {
                      Navigator.pop(context);
                      _openTerminal(context);
                    }
                  : null,
            ),
            if (isRunning)
              ListTile(
                leading: const Icon(Icons.stop, color: Colors.orange),
                title: const Text('Stop Container'),
                onTap: () async {
                  Navigator.pop(context);
                  await appState.stopContainer(container.name);
                },
              ),
            ListTile(
              leading: const Icon(Icons.system_update_alt),
              title: const Text('Upgrade Container'),
              onTap: () async {
                Navigator.pop(context);
                await appState.upgradeContainer(container.name);
              },
            ),
            const Divider(),
            ListTile(
              leading: const Icon(Icons.delete, color: Colors.red),
              title:
                  const Text('Delete Container', style: TextStyle(color: Colors.red)),
              onTap: () {
                Navigator.pop(context);
                _confirmDelete(context);
              },
            ),
          ],
        ),
      ),
    );
  }

  void _confirmDelete(BuildContext context) {
    showDialog(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Delete Container'),
        content: Text(
            'Are you sure you want to delete "${container.name}"? This action cannot be undone.'),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: Colors.red),
            onPressed: () async {
              Navigator.pop(context);
              await context.read<AppStateProvider>().removeContainer(container.name);
            },
            child: const Text('Delete'),
          ),
        ],
      ),
    );
  }

  void _openDetails(BuildContext context) {
    context.read<AppStateProvider>().selectContainer(container);
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => ContainerDetailsPage(container: container),
      ),
    );
  }

  void _openTerminal(BuildContext context) {
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (context) => ContainerTerminalPage(container: container),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final statusColor = _getStatusColor(container.status);
    final statusText = _getStatusText(container.status);
    final isRunning = container.status is Status_Up;

    return Card(
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(12),
        side: BorderSide(color: Theme.of(context).dividerColor),
      ),
      margin: const EdgeInsets.only(bottom: 12),
      child: InkWell(
        borderRadius: BorderRadius.circular(12),
        onTap: () => _openDetails(context),
        onLongPress: () => _showQuickActionsMenu(context),
        child: Padding(
          padding: const EdgeInsets.all(16),
          child: Row(
            children: [
              // Container icon with status indicator
              Stack(
                children: [
                  Container(
                    width: 48,
                    height: 48,
                    decoration: BoxDecoration(
                      color: Theme.of(context).colorScheme.surfaceContainerHighest,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Icon(
                      _getDistroIcon(container.image),
                      color: Theme.of(context).colorScheme.primary,
                    ),
                  ),
                  Positioned(
                    right: 0,
                    bottom: 0,
                    child: Container(
                      width: 14,
                      height: 14,
                      decoration: BoxDecoration(
                        color: statusColor,
                        shape: BoxShape.circle,
                        border: Border.all(
                          color: Theme.of(context).cardColor,
                          width: 2,
                        ),
                      ),
                    ),
                  ),
                ],
              ),
              const SizedBox(width: 16),
              // Container info
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      container.name,
                      style: const TextStyle(
                        fontWeight: FontWeight.bold,
                        fontSize: 16,
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      statusText,
                      style: TextStyle(
                        color: statusColor,
                        fontSize: 12,
                        fontWeight: FontWeight.w500,
                      ),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      container.image,
                      style: TextStyle(
                        color: Theme.of(context).hintColor,
                        fontSize: 11,
                      ),
                      overflow: TextOverflow.ellipsis,
                    ),
                  ],
                ),
              ),
              // Action buttons
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (isRunning)
                    IconButton.filledTonal(
                      icon: const Icon(Icons.terminal, size: 20),
                      tooltip: 'Open Terminal',
                      onPressed: () => _openTerminal(context),
                    ),
                  IconButton(
                    icon: const Icon(Icons.more_vert),
                    tooltip: 'More Actions',
                    onPressed: () => _showQuickActionsMenu(context),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}
