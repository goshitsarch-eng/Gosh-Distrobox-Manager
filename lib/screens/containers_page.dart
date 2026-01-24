import 'package:flutter/material.dart';
import 'package:gosh_distrobox_manager/screens/create_container_page.dart';
import 'package:provider/provider.dart';
import 'package:gosh_distrobox_manager/providers/app_state.dart';
import 'package:gosh_distrobox_manager/widgets/container_card.dart';

class ContainersPage extends StatelessWidget {
  const ContainersPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: const Text('Containers'),
        actions: [
          IconButton(
            icon: const Icon(Icons.refresh),
            onPressed: () {
              context.read<AppStateProvider>().refresh();
            },
          ),
        ],
      ),
      body: Consumer<AppStateProvider>(
        builder: (context, appState, child) {
          if (appState.isLoading) {
            return const Center(child: CircularProgressIndicator());
          }
          if (!appState.isDistroboxInstalled) {
            return Center(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  const Icon(Icons.warning_amber_rounded, size: 64, color: Colors.orange),
                  const SizedBox(height: 16),
                  const Text(
                    'Distrobox Not Found',
                    style: TextStyle(fontSize: 24, fontWeight: FontWeight.bold),
                  ),
                  const SizedBox(height: 8),
                  const Text('Please install Distrobox to use this application.'),
                  const SizedBox(height: 16),
                  FilledButton.tonal(
                    onPressed: () => appState.refresh(),
                    child: const Text('Refresh'),
                  ),
                ],
              ),
            );
          }
          if (appState.error != null) {
            return Center(child: Text('Error: ${appState.error}'));
          }
          if (appState.containers.isEmpty) {
            return const Center(child: Text('No containers found.'));
          }
          return ListView.builder(
            padding: const EdgeInsets.all(16),
            itemCount: appState.containers.length,
            itemBuilder: (context, index) {
              final container = appState.containers[index];
              return ContainerCard(container: container);
            },
          );
        },
      ),
      floatingActionButton: FloatingActionButton.extended(
        onPressed: () {
          Navigator.of(context).push(
            MaterialPageRoute(builder: (context) => const CreateContainerPage()),
          );
        },
        label: const Text('New Container'),
        icon: const Icon(Icons.add),
      ),
    );
  }
}
